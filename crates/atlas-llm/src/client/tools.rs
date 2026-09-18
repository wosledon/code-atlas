use anyhow::Result;
use std::collections::HashSet;
use std::time::Instant;

use crate::types::{LlmResponse, LlmTurn, LlmUsage, ToolCall, ToolSpec};
use crate::wire::{ChatMessage, ToolCallFunction, ToolCallPayload};

use super::{LlmClient, dialect, sse, truncate};

/// Calls answered per round. Every result stays in the history and is re-sent on
/// each later round, so breadth here is paid for 2×.
const CALLS_PER_ROUND: usize = 3;

/// How far past the read budget a page may go while it keeps writing, as a
/// multiple of that budget.
///
/// A streaming model writes *while* it reads, so a round that adds page text and
/// asks for a file is progress: charging it would cut the page off mid-sentence
/// once the model has read as many files as the budget allows. Only rounds that
/// read and produce nothing are charged. Every round still re-sends the whole
/// conversation, so this is the cost ceiling that bounds a page — it belongs to
/// the config knob (`max_tool_rounds × CALLS_PER_ROUND × this`) rather than to a
/// fixed number, so a repository that needs deeper reading can raise it.
const PROGRESS_CEILING: usize = 4;

/// Nudges injected into the conversation when a round goes nowhere (no new page
/// text, no call that was not made before), before the page is given up on.
///
/// A live-but-stuck model is usually repeating itself, not refusing: it re-reads
/// a file it already read, or re-sends the paragraph it already wrote. Telling it
/// what it just did — in the conversation it is about to re-send anyway — is what
/// breaks the loop, and it costs no extra call. Truncating instead would throw
/// away a page that one instruction could have saved.
///
/// The last entry is deliberately blunt: stop calling tools, write the page now.
const STUCK_NUDGES: &[&str] = &[
    "系统提示：你刚才那一轮没有推进——你重复了已经做过的工具调用，或重复了已经写过的内容。\
不要再重复它。请基于上面已经读到的内容继续写页面正文；如果确实还缺信息，就读取一个**还没读过**的文件、或换一个搜索词、或缩小行范围。",
    "系统提示（最后一次）：停止一切工具调用，立即输出完整的页面 markdown 正文。\
你需要的材料已经在上面了。不要重复已写过的段落，不要重新读取已经读过的文件。",
];

/// Rounds spent re-telling the model that its tool budget is gone, before the
/// call gives up and returns whatever prose it produced. The tool result is the
/// signal that makes a model stop reading and write; simply not offering tools
/// in the request is one a dialect-speaking model ignores.
const BUDGET_REFUSALS: usize = 2;

/// Answered instead of a tool once the page's budget is spent.
const BUDGET_SPENT: &str = "ERROR: 本页的工具调用预算已用尽，不会再执行新的工具调用。\
请立即用已经读到的内容直接输出页面 markdown 正文，不要再输出任何工具调用标签（<tool_call> / <function=）。";

/// Answered for a call identical to one already executed in this conversation.
/// Re-running it would burn a round to re-send bytes the model has already seen.
const DUPLICATE_CALL: &str = "（这次调用与本次对话中已执行过的调用完全相同：结果就在上面对应的工具消息里，\
不会重复执行。请直接使用那份结果继续写作；需要更多内容时，换一个文件、行范围或搜索词。）";

/// Identity of a tool call for duplicate detection: the same call written with
/// its arguments in a different order is the same call.
fn call_key(name: &str, args: &str) -> String {
    let normalized = serde_json::from_str::<serde_json::Value>(args)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| args.trim().to_string());
    format!("{name}\u{1}{normalized}")
}

/// What folding one round's prose into the page did.
struct Merged {
    /// The page gained content, so this round moved it forward.
    advanced: bool,
    /// What was just streamed is new text for the draft on disk: replacing it
    /// (a superset of what is already there) would duplicate the page in the
    /// preview, and a repeat has nothing to add.
    keep_stream: bool,
}

/// Fold one round's prose into the page being written.
///
/// A streaming model writes while it reads: the round that asks for a file can
/// carry real page text, and dropping it is what made pages restart after every
/// read. Two shapes are not page text:
/// - a tool round whose prose has no markdown structure at all is narration
///   ("让我先读一下 X"), so it is not pasted on top of the page;
/// - text that restates what is already there (models re-send their answer,
///   whole or as a tail, after a tool result) replaces it instead of being
///   appended, which would duplicate the page.
fn merge_prose(body: &mut String, prose: &str, is_tool_round: bool) -> Merged {
    let dropped = Merged {
        advanced: false,
        keep_stream: false,
    };
    let prose = prose.trim();
    if prose.is_empty() {
        return dropped;
    }
    if is_tool_round && !looks_like_page(prose) {
        return dropped;
    }
    let current = body.trim();
    if current.is_empty() {
        *body = prose.to_string();
        return Merged {
            advanced: true,
            keep_stream: true,
        };
    }
    if prose == current || current.ends_with(prose) {
        return dropped;
    }
    if prose.contains(current) {
        *body = prose.to_string();
        return Merged {
            advanced: true,
            keep_stream: false,
        };
    }
    if !body.ends_with('\n') {
        body.push('\n');
    }
    // A continued sentence must not be split, but a new block (heading, table,
    // list) needs the blank line markdown gives it everywhere else.
    if looks_like_page(prose) && !body.ends_with("\n\n") {
        body.push('\n');
    }
    body.push_str(prose);
    Merged {
        advanced: true,
        keep_stream: true,
    }
}

/// Whether `text` already carries the shape of a page (a heading, table, list,
/// fence, quote) rather than a line about the work.
fn looks_like_page(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with('#')
            || t.starts_with('|')
            || t.starts_with("- ")
            || t.starts_with("* ")
            || t.starts_with("```")
            || t.starts_with('>')
    })
}

impl LlmClient {
    /// Ask the model to write something while letting it call read-only tools
    /// (`read_file`, `list_files`, `grep`, ...) to pull in only the source it
    /// needs, instead of receiving the whole repository up front.
    ///
    /// Reading and writing are not separate phases: a streaming model keeps
    /// writing while it reads, so prose and tool calls arrive in the same round.
    /// The returned text is every prose segment in order — the page it was
    /// writing ([`merge_prose`]) — not just the last round's answer.
    ///
    /// `max_rounds` bounds the reading: it is the number of `CALLS_PER_ROUND`
    /// batches a page may spend on rounds that wrote *nothing*. Rounds that also
    /// added page text keep their reads (up to [`PROGRESS_CEILING`]), because
    /// cutting those off is what leaves a page half-written.
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
        // Reading costs context, so it is bounded — but bounded by the rounds
        // that read *without writing*. `max_rounds` is what a page needs to read
        // (the brief asks for 3–6 files), and models that ask for one file per
        // round instead of batching them are the common case. A round that also
        // wrote page text is progress and its reads are not charged; only the
        // ceiling stops those, because every round re-sends the conversation.
        let budget = max_rounds * CALLS_PER_ROUND;
        let ceiling = budget * PROGRESS_CEILING;
        let mut idle_calls = 0usize;
        let mut total_calls = 0usize;
        let mut refusals = 0usize;
        let mut stuck = 0usize;
        let mut noted_progress = false;
        let mut round = 0usize;
        // Every call this page already executed, so the next copy of it can be
        // answered from the conversation instead of run again.
        let mut seen: HashSet<String> = HashSet::new();
        // The page as it is being written. A streaming model keeps writing while
        // it reads, so prose and tool calls arrive in the same round and this
        // accumulates across rounds instead of being reset by the call that
        // interrupted the writing.
        let mut body = String::new();

        loop {
            // Tools are offered while the page is inside its budgets; the model
            // is told why in the answer to the call it asks for anyway (below).
            let offered = idle_calls <= budget && total_calls < ceiling;
            let offer: &[ToolSpec] = if offered { tools } else { &[] };
            sse::emit_round_start();
            let LlmTurn {
                text: raw,
                tool_calls,
                usage,
                model,
                // Billing detail for the run summary; the loop only needs the
                // text, the calls and the token counts it accumulates.
                cached_tokens: _,
            } = self.post_chat(&messages, offer, started.elapsed()).await?;
            prompt_tokens += usage.prompt_tokens;
            completion_tokens += usage.completion_tokens;
            // Some models ask for tools as text (`<tool_call><function=read_file>
            // <parameter=path>…`) rather than through `tool_calls[]`; run what
            // they asked for so the transcript never reaches the caller.
            let (calls, prose) = dialect::resolve(&raw, tool_calls, tools, &round.to_string());
            round += 1;
            let is_tool_round = !calls.is_empty();
            let merged = merge_prose(&mut body, &prose, is_tool_round);
            // Keep what was streamed in the draft on disk only when it is new
            // page text: a transcript we could not run, a repeated paragraph or
            // a re-sent page must not pile up there either.
            sse::emit_round_end(merged.keep_stream && prose == raw);
            if !is_tool_round {
                return Ok(LlmResponse {
                    text: body,
                    usage: LlmUsage {
                        prompt_tokens,
                        completion_tokens,
                        latency_ms: started.elapsed().as_millis() as i64,
                    },
                    model,
                });
            }
            // Split the requested calls into ones that are new to this page and
            // ones the model is asking for again; only the new ones are run.
            let mut marked: Vec<(ToolCall, bool)> = Vec::new();
            for call in calls.into_iter().take(CALLS_PER_ROUND) {
                let fresh = seen.insert(call_key(&call.name, &call.arguments));
                marked.push((call, fresh));
            }
            let has_fresh = marked.iter().any(|(_, fresh)| *fresh);
            // A live but going-nowhere round: nothing new written, nothing new
            // asked for. Tell the model what it just did and let it recover —
            // the nudge rides along with the request it is sent anyway.
            let mut nudge: Option<&str> = None;
            if !merged.advanced && !has_fresh {
                if stuck < STUCK_NUDGES.len() {
                    nudge = Some(STUCK_NUDGES[stuck]);
                    stuck += 1;
                    tracing::info!(
                        "第 {round} 轮无进展（重复调用 / 重复正文），注入第 {stuck}/{} 次提示词纠偏",
                        STUCK_NUDGES.len()
                    );
                } else {
                    tracing::warn!(
                        "模型连续 {} 轮没有新内容（重复调用 / 重复正文），提示词纠偏无效，结束本页（第 {round} 轮）",
                        STUCK_NUDGES.len() + 1
                    );
                    return Ok(LlmResponse {
                        text: body,
                        usage: LlmUsage {
                            prompt_tokens,
                            completion_tokens,
                            latency_ms: started.elapsed().as_millis() as i64,
                        },
                        model,
                    });
                }
            } else {
                stuck = 0;
            }
            // A round that added page text has moved the page forward, so its
            // reads are granted past the read budget; a round that only read is
            // charged, and runs out first. The ceiling applies to both; repeated
            // calls are answered from the conversation and cost nothing.
            let allow_calls = total_calls < ceiling && (merged.advanced || idle_calls < budget);
            if !allow_calls && has_fresh {
                // `max_rounds = 0` disables tools outright: nothing was promised,
                // so nothing is refused.
                let allowed = if budget == 0 { 0 } else { BUDGET_REFUSALS };
                refusals += 1;
                if refusals > allowed {
                    if budget > 0 {
                        let why = if total_calls >= ceiling {
                            format!("已达每页 {ceiling} 次调用上限")
                        } else {
                            format!("已读 {idle_calls} 次未写正文（读取预算 {budget} 次）")
                        };
                        tracing::warn!("工具调用停止：{why}，模型仍未写正文");
                    }
                    return Ok(LlmResponse {
                        text: body,
                        usage: LlmUsage {
                            prompt_tokens,
                            completion_tokens,
                            latency_ms: started.elapsed().as_millis() as i64,
                        },
                        model,
                    });
                }
                tracing::info!(
                    "工具调用停止（{total_calls} 次调用 / {idle_calls} 次未写正文），模型仍请求工具：答复「直接写正文」（第 {refusals}/{allowed} 次）"
                );
            } else if merged.advanced && idle_calls >= budget && !noted_progress {
                // Worth saying out loud: this is the page that used to be cut
                // off mid-write for reading too many files.
                noted_progress = true;
                tracing::info!(
                    "模型边写边读：已用 {total_calls} 次调用（读取预算 {budget} 次只对「只读不写」计费），继续写直到 {ceiling} 次上限"
                );
            }
            // Echo back exactly the calls that get answered below, or the next
            // request carries `tool_calls` no message replies to.
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: (!prose.trim().is_empty()).then_some(prose),
                tool_calls: Some(
                    marked
                        .iter()
                        .map(|(c, _)| ToolCallPayload {
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
            for (call, fresh) in &marked {
                let result = if !fresh {
                    // The model asked again for something it already has: the
                    // answer is in the conversation it just re-sent.
                    DUPLICATE_CALL.to_string()
                } else if allow_calls && total_calls < ceiling {
                    // Re-check the ceiling per call: a round that starts inside
                    // it must not step past it by answering three files at once.
                    total_calls += 1;
                    if !merged.advanced {
                        idle_calls += 1;
                    }
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
            // After the tool replies (an assistant `tool_calls` message must be
            // followed by them), the corrective instruction closes the round.
            if let Some(nudge) = nudge {
                messages.push(ChatMessage::user(nudge));
            }
        }
    }
}
