//! SSE (`text/event-stream`) accumulation for OpenAI-compatible chat streams.

use crate::types::LlmTurn;
use crate::wire::{StreamChunk, StreamToolCallDelta};
use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::sync::Arc;

// Per-task sink for streamed text deltas (progress bars, live page previews).
// Task-local so concurrent page generations each see their own sink.
tokio::task_local! {
    static STREAM_SINK: Arc<dyn StreamSink>;
}

/// Where the text a model streams goes while it works.
///
/// Marks nest (one per tool round, one per HTTP attempt inside it): `round_start`
/// remembers a position and `round_end(false)` drops everything written since —
/// so phrasing that preceded a tool call, or the partial text of an attempt that
/// is being retried, never lands in a live preview or a half-written page.
pub trait StreamSink: Send + Sync {
    /// One streamed content delta.
    fn delta(&self, text: &str);
    /// A round (or HTTP attempt) started; text from here on is provisional.
    fn round_start(&self) {}
    /// The round ended; `kept == false` rolls back to the matching `round_start`.
    fn round_end(&self, kept: bool) {
        let _ = kept;
    }
}

/// Wrap a closure as a sink that ignores round boundaries.
pub fn sink_fn<F>(f: F) -> Arc<dyn StreamSink>
where
    F: Fn(&str) + Send + Sync + 'static,
{
    struct FnSink<F>(F);
    impl<F: Fn(&str) + Send + Sync> StreamSink for FnSink<F> {
        fn delta(&self, text: &str) {
            (self.0)(text)
        }
    }
    Arc::new(FnSink(f))
}

/// Feed two sinks at once: the page's progress bar and the file on disk.
pub fn sink_pair(a: Arc<dyn StreamSink>, b: Arc<dyn StreamSink>) -> Arc<dyn StreamSink> {
    struct Pair(Arc<dyn StreamSink>, Arc<dyn StreamSink>);
    impl StreamSink for Pair {
        fn delta(&self, text: &str) {
            self.0.delta(text);
            self.1.delta(text);
        }
        fn round_start(&self) {
            self.0.round_start();
            self.1.round_start();
        }
        fn round_end(&self, kept: bool) {
            self.0.round_end(kept);
            self.1.round_end(kept);
        }
    }
    Arc::new(Pair(a, b))
}

/// Run `fut` with every streamed content delta forwarded to `sink`.
pub async fn with_stream_sink<F, T>(sink: Arc<dyn StreamSink>, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    STREAM_SINK.scope(sink, fut).await
}

fn emit_delta(text: &str) {
    let _ = STREAM_SINK.try_with(|sink| sink.delta(text));
}

/// Open a mark: everything streamed until the matching [`emit_round_end`] is
/// provisional.
pub(crate) fn emit_round_start() {
    let _ = STREAM_SINK.try_with(|sink| sink.round_start());
}

/// Close the innermost mark; `kept == false` discards what it produced.
pub(crate) fn emit_round_end(kept: bool) {
    let _ = STREAM_SINK.try_with(|sink| sink.round_end(kept));
}

#[derive(Default)]
struct ToolBuilder {
    id: String,
    name: String,
    arguments: String,
}

/// Fold a streaming chat completion into one final [`LlmTurn`].
#[derive(Default)]
pub(crate) struct StreamAccum {
    text: String,
    tools: BTreeMap<usize, ToolBuilder>,
    prompt_tokens: i64,
    completion_tokens: i64,
    model: String,
}

impl StreamAccum {
    pub fn apply_chunk(&mut self, chunk: &StreamChunk) {
        if let Some(usage) = &chunk.usage {
            self.prompt_tokens = usage.prompt_tokens.unwrap_or(0);
            self.completion_tokens = usage.completion_tokens.unwrap_or(0);
        }
        let Some(choice) = chunk.choices.as_ref().and_then(|c| c.first()) else {
            return;
        };
        let Some(delta) = &choice.delta else {
            return;
        };
        if let Some(content) = &delta.content {
            if !content.is_empty() {
                emit_delta(content);
            }
            self.text.push_str(content);
        }
        for tc in delta.tool_calls.iter().flatten() {
            self.apply_tool_delta(tc);
        }
    }

    fn apply_tool_delta(&mut self, tc: &StreamToolCallDelta) {
        let idx = tc.index.unwrap_or(0);
        // Text that arrived before this call stays in `text`: a streaming model
        // writes the page while it reads, so that text is page content, not
        // narration. The caller decides whether it is worth keeping.
        let slot = self.tools.entry(idx).or_default();
        if let Some(id) = &tc.id
            && !id.is_empty()
        {
            slot.id = id.clone();
        }
        if let Some(f) = &tc.function {
            if let Some(n) = &f.name
                && !n.is_empty()
            {
                if slot.name.is_empty() {
                    slot.name = n.clone();
                } else {
                    slot.name.push_str(n);
                }
            }
            if let Some(a) = &f.arguments {
                slot.arguments.push_str(a);
            }
        }
    }

    pub fn finish(mut self, fallback_model: &str, latency_ms: i64) -> LlmTurn {
        if self.model.is_empty() {
            self.model = fallback_model.to_string();
        }
        let mut tool_calls: Vec<crate::types::ToolCall> = self
            .tools
            .into_iter()
            .filter(|(_, t)| !t.name.is_empty())
            .enumerate()
            .map(|(i, (_, t))| crate::types::ToolCall {
                id: if t.id.is_empty() {
                    format!("call_{i}")
                } else {
                    t.id
                },
                name: t.name,
                arguments: if t.arguments.trim().is_empty() {
                    "{}".into()
                } else {
                    t.arguments
                },
            })
            .collect();
        tool_calls.sort_by(|a, b| a.id.cmp(&b.id));
        LlmTurn {
            text: self.text,
            tool_calls,
            usage: crate::types::LlmUsage {
                prompt_tokens: self.prompt_tokens,
                completion_tokens: self.completion_tokens,
                latency_ms,
            },
            model: self.model,
        }
    }
}

/// Read an SSE body from a live response and fold every `data:` payload.
pub(crate) async fn read_openai_sse(mut resp: reqwest::Response) -> Result<StreamAccum> {
    let mut acc = StreamAccum::default();
    let mut buf = String::new();
    loop {
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| anyhow!("llm stream read error: {e}"))?;
        let Some(bytes) = chunk else {
            break;
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim_end_matches('\r').to_string();
            buf.drain(..=pos);
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            let parsed: StreamChunk = serde_json::from_str(payload).map_err(|e| {
                anyhow!(
                    "llm stream chunk is not valid JSON: {e}; body={}",
                    truncate(payload, 200)
                )
            })?;
            acc.apply_chunk(&parsed);
        }
    }
    Ok(acc)
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{StreamChoice, StreamDelta, StreamToolFnDelta};

    #[test]
    fn folds_text_and_tool_call_deltas() {
        let mut acc = StreamAccum::default();
        acc.apply_chunk(&StreamChunk {
            choices: Some(vec![StreamChoice {
                delta: Some(StreamDelta {
                    content: Some("Hel".into()),
                    tool_calls: None,
                }),
            }]),
            usage: None,
        });
        acc.apply_chunk(&StreamChunk {
            choices: Some(vec![StreamChoice {
                delta: Some(StreamDelta {
                    content: Some("lo".into()),
                    tool_calls: Some(vec![StreamToolCallDelta {
                        index: Some(0),
                        id: Some("call_a".into()),
                        function: Some(StreamToolFnDelta {
                            name: Some("read_".into()),
                            arguments: None,
                        }),
                    }]),
                }),
            }]),
            usage: None,
        });
        acc.apply_chunk(&StreamChunk {
            choices: Some(vec![StreamChoice {
                delta: Some(StreamDelta {
                    content: None,
                    tool_calls: Some(vec![StreamToolCallDelta {
                        index: Some(0),
                        id: None,
                        function: Some(StreamToolFnDelta {
                            name: Some("file".into()),
                            arguments: Some("{\"path\"".into()),
                        }),
                    }]),
                }),
            }]),
            usage: Some(crate::wire::UsageBody {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
            }),
        });
        acc.apply_chunk(&StreamChunk {
            choices: Some(vec![StreamChoice {
                delta: Some(StreamDelta {
                    content: None,
                    tool_calls: Some(vec![StreamToolCallDelta {
                        index: Some(0),
                        id: None,
                        function: Some(StreamToolFnDelta {
                            name: None,
                            arguments: Some(":\"a.rs\"}".into()),
                        }),
                    }]),
                }),
            }]),
            usage: None,
        });
        let turn = acc.finish("m", 42);
        assert_eq!(turn.text, "Hello");
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "read_file");
        assert_eq!(turn.tool_calls[0].arguments, "{\"path\":\"a.rs\"}");
        assert_eq!(turn.usage.prompt_tokens, 10);
        assert_eq!(turn.usage.completion_tokens, 5);
        assert_eq!(turn.usage.latency_ms, 42);
    }
}
