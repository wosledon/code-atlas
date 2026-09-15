use anyhow::Result;
use std::time::Instant;

use crate::types::{LlmResponse, LlmUsage, ToolSpec};
use crate::wire::{ChatMessage, ToolCallFunction, ToolCallPayload};

use super::{truncate, LlmClient};

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
        let mut last_text = String::new();

        for round in 0..=max_rounds {
            // Last round is tool-free, forcing the model to answer with text.
            let offer: &[ToolSpec] = if round < max_rounds { tools } else { &[] };
            let turn = self.post_chat(&messages, offer, started.elapsed()).await?;
            prompt_tokens += turn.usage.prompt_tokens;
            completion_tokens += turn.usage.completion_tokens;
            if !turn.text.trim().is_empty() {
                last_text = turn.text.clone();
            }
            if turn.tool_calls.is_empty() {
                return Ok(LlmResponse {
                    text: turn.text,
                    usage: LlmUsage {
                        prompt_tokens,
                        completion_tokens,
                        latency_ms: started.elapsed().as_millis() as i64,
                    },
                    model: turn.model,
                });
            }
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: if turn.text.trim().is_empty() { None } else { Some(turn.text.clone()) },
                tool_calls: Some(
                    turn.tool_calls
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
            for call in &turn.tool_calls {
                let result = match exec(&call.name, &call.arguments) {
                    Ok(out) => out,
                    Err(e) => format!("ERROR: {e:#}"),
                };
                messages.push(ChatMessage::tool(&truncate(&result, 20_000), &call.id));
            }
        }
        // Unreachable in practice: the final iteration is tool-free.
        Ok(LlmResponse {
            text: last_text,
            usage: LlmUsage {
                prompt_tokens,
                completion_tokens,
                latency_ms: started.elapsed().as_millis() as i64,
            },
            model: self.cfg.model.clone(),
        })
    }
}
