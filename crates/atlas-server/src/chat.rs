use super::*;
use crate::common::{open_project_store, resolve_project};
use crate::search::search_mode_for;
use atlas_core::tools::RepoTools;
use axum::body::Body;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

mod sink;
mod tools;

use sink::AnswerSink;
use tools::{answer, chat_tools};

#[derive(serde::Deserialize)]
pub(crate) struct ChatMessageIn {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct ChatRequest {
    messages: Vec<ChatMessageIn>,
    #[serde(default)]
    top_k: Option<i64>,
    /// Project id; empty → launch project.
    #[serde(default)]
    project: Option<String>,
}

/// Streaming knowledge-base chat.
///
/// SSE frames (each `data: <json>\n\n`):
/// - `{"type":"meta","project","mode","sources","contexts"}`
/// - `{"type":"delta","text"}` — answer tokens while the LLM streams
/// - `{"type":"reset","text"}` — take back the provisional text, keep `text`
/// - `{"type":"done","project","mode","answer","sources","contexts","usage"?}` — final
pub(crate) async fn kb_chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> Response {
    let user_msg = body
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();
    if user_msg.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "empty question"})),
        )
            .into_response();
    }

    let pref = match resolve_project(&state, body.project.as_deref()) {
        Ok(p) => p,
        Err(e) => return crate::common::err(e),
    };
    let store = match open_project_store(&pref) {
        Ok(s) => s,
        Err(e) => return crate::common::err(e),
    };
    let top_k = body.top_k.unwrap_or(8);
    let hits = match store.search(&user_msg, top_k, search_mode_for(&pref)) {
        Ok(h) => h,
        Err(e) => return crate::common::err(e),
    };

    let sources: Vec<serde_json::Value> = hits
        .iter()
        .map(|h| {
            json!({
                "kind": h.kind,
                "id": h.id,
                "title": h.title,
                "summary": h.summary,
                "page_path": h.page_path,
                "score": h.score,
            })
        })
        .collect();
    let contexts: Vec<serde_json::Value> = hits
        .iter()
        .map(|h| {
            json!({
                "kind": h.kind,
                "id": h.id,
                "title": h.title,
                "page_path": h.page_path,
                "summary": h.summary,
                "body": h.body.clone().unwrap_or_default(),
                "score": h.score,
                "start_line": h.start_line,
                "end_line": h.end_line,
            })
        })
        .collect();

    let retrieval_answer = format_hits_as_answer(&user_msg, &hits);

    let llm = atlas_llm::LlmClient::from_env(atlas_llm::LlmConfig {
        provider: pref.cfg.llm.provider.clone(),
        model: pref.cfg.llm.model.clone(),
        api_key: pref.cfg.llm.api_key.clone(),
        base_url: pref.cfg.llm.base_url.clone(),
        temperature: 0.2,
        max_output_tokens: 2048,
        timeout_secs: pref.cfg.llm.timeout_secs.min(30),
        retries: 0,
    });
    let llm = match llm {
        Ok(c) if c.configured() && !c.is_host_agent() => Some(c),
        _ => None,
    };

    // Repository access for the answer. A failed setup (unreadable root) is not
    // fatal: the answer then rests on chunk recall alone, as it did before.
    let tools = RepoTools::new(
        &pref.root,
        pref.cfg.privacy.redact_paths.clone(),
        pref.cfg.privacy.max_file_bytes,
    )
    .map(Arc::new)
    .map_err(|e| tracing::warn!("仓库工具不可用，本次对话仅依赖召回：{e:#}"))
    .ok();
    let specs = chat_tools();
    let rounds = pref.cfg.llm.max_tool_rounds;

    // Build the grounded prompt only when an LLM is available.
    let language = pref.cfg.output.language.clone();
    let project_id = pref.id.clone();
    let prompt = llm.as_ref().map(|_| {
        let mut context = String::new();
        for (i, h) in hits.iter().enumerate() {
            let where_ = h
                .page_path
                .as_deref()
                .map(|p| match (h.start_line, h.end_line) {
                    (Some(a), Some(b)) => format!("{p}:{a}-{b}"),
                    _ => p.to_string(),
                })
                .unwrap_or_else(|| "(repository graph)".into());
            context.push_str(&format!("### [{}] {} · {}\n", i + 1, h.kind, where_));
            context.push_str(&format!("Title: {}\nSummary: {}\n", h.title, h.summary));
            if let Some(b) = h.body.as_deref() {
                let clipped: String = b.chars().take(2400).collect();
                context.push_str(&format!("Content:\n{clipped}\n"));
            }
            context.push('\n');
        }
        let history: Vec<String> = body
            .messages
            .iter()
            .take(body.messages.len().saturating_sub(1))
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect();
        let mut user = String::new();
        if !history.is_empty() {
            user.push_str("Conversation so far:\n");
            user.push_str(&history.join("\n"));
            user.push_str("\n\n");
        }
        user.push_str(&format!("Question:\n{user_msg}"));
        let system = format!(
            "You are Code Atlas assistant. Answer in {}. \
             Use ONLY the provided repository/wiki context for factual claims. \
             Cite page paths like `atlas/architecture/overview.md` when used.\n\
             The recalled context above is ranked by similarity, so it can be incomplete or \
             stale: before answering that something is missing, call `grep` to locate the \
             symbol, config key or message in the repository and `read_file` to read around a \
             match. Say the context is insufficient only after searching.\n\
             When a flow, a layering or a data relationship is clearer as a diagram, \
             include ONE ```mermaid block (flowchart / sequenceDiagram). \
             Its syntax must parse: node ids are plain ASCII ids and never a mermaid \
             keyword (`graph`, `end`, `subgraph`, `class`, `style`, `click`, `direction`, \
             `default`); wrap EVERY node and edge label in double quotes and use `<br/>` \
             instead of a literal newline, e.g. `a[\"crates/atlas-core<br/>run_init\"]`, \
             `a -->|\"是\"| b`. An unquoted label is a parse error.\n\n## Context\n{}",
            language, context
        );
        (system, user)
    });

    let (tx, rx) = mpsc::channel::<String>(128);
    tokio::spawn(async move {
        let emit = |v: serde_json::Value| format!("data: {}\n\n", v);
        let sources_meta = sources.clone();
        let contexts_meta = contexts.clone();
        let _ = tx
            .send(emit(json!({
                "type": "meta",
                "project": project_id,
                "mode": if llm.is_some() { "llm" } else { "retrieval" },
                "sources": sources_meta,
                "contexts": contexts_meta,
            })))
            .await;

        let Some(llm) = llm else {
            let _ = tx
                .send(emit(json!({
                    "type": "done",
                    "project": project_id,
                    "mode": "retrieval",
                    "answer": retrieval_answer,
                    "sources": sources,
                    "contexts": contexts,
                })))
                .await;
            return;
        };

        let (system, user) = prompt.expect("prompt built with llm");
        let sink = AnswerSink::new(tx.clone());
        let result = atlas_llm::with_stream_sink(
            sink,
            answer(&llm, tools.as_deref(), &specs, rounds, &system, &user),
        )
        .await;
        let frame = match result {
            Ok(resp) if !resp.text.trim().is_empty() => json!({
                "type": "done",
                "project": project_id,
                "mode": "llm",
                "answer": resp.text,
                "model": resp.model,
                "sources": sources,
                "contexts": contexts,
                "usage": {
                    "prompt_tokens": resp.usage.prompt_tokens,
                    "completion_tokens": resp.usage.completion_tokens,
                }
            }),
            Ok(resp) => {
                tracing::warn!("kb chat llm returned empty text, falling back to retrieval");
                json!({
                    "type": "done",
                    "project": project_id,
                    "mode": "retrieval",
                    "answer": retrieval_answer,
                    "sources": sources,
                    "contexts": contexts,
                    "model": resp.model,
                })
            }
            Err(e) => {
                tracing::warn!("kb chat llm failed, falling back to retrieval: {e:#}");
                json!({
                    "type": "done",
                    "project": project_id,
                    "mode": "retrieval",
                    "answer": retrieval_answer,
                    "sources": sources,
                    "contexts": contexts,
                    "error": format!("{e:#}"),
                })
            }
        };
        let _ = tx.send(emit(frame)).await;
    });

    let stream = ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>);
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|e| {
            tracing::error!("sse response build failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "sse build failed").into_response()
        })
}

pub(crate) fn format_hits_as_answer(question: &str, hits: &[SearchHit]) -> String {
    let mut md = String::new();
    md.push_str(&format!(
        "**已退回知识库检索结果**（未使用模型综合回答；问题：「{}」）。\n\n",
        question
    ));
    if hits.is_empty() {
        md.push_str(
            "没有命中相关内容。可先运行 `atlas init` / `atlas update`，或换更短的关键词。\n",
        );
        return md;
    }
    md.push_str("### 相关材料\n\n");
    for (i, h) in hits.iter().enumerate() {
        md.push_str(&format!("{}. **{}**", i + 1, h.title));
        if let Some(p) = &h.page_path {
            md.push_str(&format!(" · `{}`", p));
        }
        md.push('\n');
        let s = h.summary.trim();
        if !s.is_empty() {
            md.push_str(&format!("   {}\n", s.replace('\n', " ")));
        }
        md.push('\n');
    }
    md.push_str("_在设置里配置 Provider / Base URL / API Key 后，可得到 LLM 综合回答。_\n");
    md
}
