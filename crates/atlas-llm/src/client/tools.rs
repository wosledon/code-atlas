use anyhow::Result;
use std::time::Instant;

use crate::types::{LlmResponse, LlmTurn, LlmUsage, ToolCall, ToolSpec};
use crate::wire::{ChatMessage, ToolCallFunction, ToolCallPayload};

use super::{LlmClient, dialect, sse, truncate};

/// Calls answered per round. Every result stays in the history and is re-sent on
/// each later round, so breadth here is paid for 2×.
const CALLS_PER_ROUND: usize = 3;

/// Rounds spent re-telling the model that its tool budget is gone, before the
/// call gives up and returns whatever prose it produced. The tool result is the
/// signal that makes a model stop reading and write; simply not offering tools
/// in the request is one a dialect-speaking model ignores.
const BUDGET_REFUSALS: usize = 2;

/// Answered instead of a tool once the page's budget is spent.
const BUDGET_SPENT: &str = "ERROR: 本页的工具调用预算已用尽，不会再执行新的工具调用。\
请立即用已经读到的内容直接输出页面 markdown 正文，不要再输出任何工具调用标签（<tool_call> / <function=）。";

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
        // The budget covers the whole page, not one round: `max_rounds` is what a
        // page needs to read (the brief asks for 3–6 files), and models that ask
        // for one file per round instead of batching them are the common case.
        // Counting *calls* is what keeps those pages from running out of source
        // and falling back to a structural template.
        let budget = max_rounds * CALLS_PER_ROUND;
        let mut executed = 0usize;
        let mut refusals = 0usize;
        let mut round = 0usize;
        // Longest prose a round produced. A model stuck on reads may still have
        // written part of the page, and that text beats the empty body a caller
        // replaces with a structural template.
        let mut best_prose = String::new();

        loop {
            // Nothing is offered once the budget is spent; the model is told why
            // in the answer to the call it asks for anyway (below).
            let offer: &[ToolSpec] = if executed < budget { tools } else { &[] };
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
            let (calls, text) = dialect::resolve(&raw, tool_calls, tools, &round.to_string());
            round += 1;
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
            if executed >= budget {
                // `max_rounds = 0` disables tools outright: nothing was promised,
                // so nothing is refused.
                let allowed = if budget == 0 { 0 } else { BUDGET_REFUSALS };
                refusals += 1;
                if refusals > allowed {
                    if budget > 0 {
                        tracing::warn!(
                            "工具调用预算已用尽（{max_rounds} 轮 × {CALLS_PER_ROUND} = {budget} 次），模型仍未写正文"
                        );
                    }
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
                    "工具调用预算已用尽（{budget} 次），模型仍请求工具：答复「直接写正文」（第 {refusals}/{allowed} 次）"
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
                let result = if executed < budget {
                    executed += 1;
                    match exec(&call.name, &call.arguments) {
                        Ok(out) => out,
                        Err(e) => format!("ERROR: {e:#}"),
                    }
                } else {
                    BUDGET_SPENT.to_string()
                };
                // Keep follow-up rounds cheap: long dumps bloat every later prompt.
                messages.push(ChatMessage::tool(&truncate(&result, 6_000), &call.id));
            }
        }
    }
}
