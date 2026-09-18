use anyhow::Result;
use std::time::Instant;

use crate::types::{LlmResponse, LlmTurn, LlmUsage, ToolCall, ToolSpec};
use crate::wire::{ChatMessage, ToolCallFunction, ToolCallPayload};

use super::{LlmClient, dialect, sse, truncate};

/// Calls answered per round. Every result stays in the history and is re-sent on
/// each later round, so breadth here is paid for 2×.
const CALLS_PER_ROUND: usize = 3;

/// Read rounds granted *after* the tool budget is spent, when the model asks for
/// a file anyway instead of writing. Running the call it asked for is what gets
/// a page written; refusing it only ever produced a structural template. Kept
/// small: every granted round is another full model call with the whole
/// conversation, and a model that still has not written after this many has
/// nothing to say about the page.
const TEXT_TOOL_ROUNDS: usize = 2;

impl LlmClient {
    /// Ask the model to write something while letting it call read-only tools
    /// (`read_file`, `list_files`, `grep`, ...) to pull in only the source it
    /// needs, instead of receiving the whole repository up front.
    ///
    /// `exec` runs one tool call and returns the text handed back to the model.
    /// Errors thrown by `exec` are reported to the model as `ERROR: ...` so it
    /// can adapt instead of failing the page.
    pub async fn chat_with_tools<F>(
        &self,
        system: &str,
        user: &str,
        tools: &[ToolSpec],
        max_rounds: usize,
        exec: F,
    ) -> Result<LlmResponse>
    where
        F: Fn(&str, &str) -> Result<String>,
    {
        if !self.supports_tools() || tools.is_empty() {
            return self.chat(system, user).await;
        }
        let started = Instant::now();
        let mut messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
        let mut prompt_tokens = 0i64;
        let mut completion_tokens = 0i64;
        let mut round = 0usize;
        let mut text_tool_rounds = 0usize;
        // Longest prose a round produced. A model stuck on reads may still have
        // written part of the page, and that text beats the empty body a caller
        // replaces with a structural template.
        let mut best_prose = String::new();

        loop {
            // Tools are offered for the budgeted rounds; after that the model is
            // asked for text, so a page cannot hang on an endless tool loop.
            let offered = round < max_rounds;
            let offer: &[ToolSpec] = if offered { tools } else { &[] };
            // Text a round emits before asking for a tool is usually narration
            // ("let me read X"); it is discarded below so it never reaches the page.
            sse::emit_round_start();
            let LlmTurn {
                text: raw,
                tool_calls,
                usage,
                model,
            } = self.post_chat(&messages, offer, started.elapsed()).await?;
            prompt_tokens += usage.prompt_tokens;
            completion_tokens += usage.completion_tokens;
            // Some models ask for tools as text (`<tool_call><function=read_file>
            // <parameter=path>…`) rather than through `tool_calls[]`; run what
            // they asked for so the transcript never reaches the caller.
            let (calls, text) = dialect::resolve(&raw, tool_calls, tools);
            if calls.is_empty() {
                // Keep the streamed text only when nothing was dropped: a
                // transcript we could not run must not stay in a draft either.
                sse::emit_round_end(text == raw);
                return Ok(LlmResponse {
                    text,
                    usage: LlmUsage {
                        prompt_tokens,
                        completion_tokens,
                        latency_ms: started.elapsed().as_millis() as i64,
                    },
                    model,
                });
            }
            sse::emit_round_end(false);
            if text.chars().count() > best_prose.chars().count() {
                best_prose = text.clone();
            }
            if !offered {
                // The model is past its tool budget and still reading, so it is
                // not writing yet. Give it the file it asked for and ask for the
                // page again — up to `TEXT_TOOL_ROUNDS` times, then take the
                // longest prose any round produced.
                text_tool_rounds += 1;
                if text_tool_rounds > TEXT_TOOL_ROUNDS {
                    tracing::warn!(
                        "工具预算已用尽（{max_rounds} 轮 + {TEXT_TOOL_ROUNDS} 次补读），模型仍未写正文"
                    );
                    return Ok(LlmResponse {
                        text: best_prose,
                        usage: LlmUsage {
                            prompt_tokens,
                            completion_tokens,
                            latency_ms: started.elapsed().as_millis() as i64,
                        },
                        model,
                    });
                }
                tracing::info!(
                    "工具预算用尽后模型仍请求工具（第 {text_tool_rounds}/{TEXT_TOOL_ROUNDS} 次），先执行再要正文"
                );
            }
            // Echo back exactly the calls that get answered below, or the next
            // request carries `tool_calls` no message replies to.
            let calls: Vec<ToolCall> = calls.into_iter().take(CALLS_PER_ROUND).collect();
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: (!text.trim().is_empty()).then_some(text),
                tool_calls: Some(
                    calls
                        .iter()
                        .map(|c| ToolCallPayload {
                            id: c.id.clone(),
                            kind: "function".into(),
                            function: ToolCallFunction {
                                name: c.name.clone(),
                                arguments: c.arguments.clone(),
                            },
                        })
                        .collect(),
                ),
                tool_call_id: None,
            });
            for call in &calls {
                let result = match exec(&call.name, &call.arguments) {
                    Ok(out) => out,
                    Err(e) => format!("ERROR: {e:#}"),
                };
                // Keep follow-up rounds cheap: long dumps bloat every later prompt.
                messages.push(ChatMessage::tool(&truncate(&result, 6_000), &call.id));
            }
            round += 1;
        }
    }
}
