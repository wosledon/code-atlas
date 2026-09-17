use super::*;
use crate::auth::{authorized, deny};
use crate::common::{open_store};
use crate::search::search_mode;
use axum::body::Body;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

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
}

/// Streaming knowledge-base chat.
///
/// SSE frames (each `data: <json>\n\n`):
/// - `{"type":"meta","mode","sources","contexts"}`
/// - `{"type":"delta","text"}` — answer tokens while the LLM streams
/// - `{"type":"done","mode","answer","sources","contexts","usage"?}` — final
pub(crate) async fn kb_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatRequest>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let user_msg = body
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();
    if user_msg.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "empty question"}))).into_response();
    }

    let store = match open_store(&state) {
        Ok(s) => s,
        Err(e) => return crate::common::err(e),
    };
    let top_k = body.top_k.unwrap_or(8);
    let hits = match store.search(&user_msg, top_k, search_mode(&state)) {
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
        provider: state.cfg.llm.provider.clone(),
        model: state.cfg.llm.model.clone(),
        api_key: state.cfg.llm.api_key.clone(),
        base_url: state.cfg.llm.base_url.clone(),
        temperature: 0.2,
        max_output_tokens: 2048,
        timeout_secs: state.cfg.llm.timeout_secs.min(30),
        retries: 0,
    });
    let llm = match llm {
        Ok(c) if c.configured() && !c.is_host_agent() => Some(c),
        _ => None,
    };

    // Build the grounded prompt only when an LLM is available.
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
             Cite page paths like `atlas/architecture/overview.md` when used. \
             If context is insufficient, say so briefly.\n\n## Context\n{}",
            state.cfg.output.language,
            context
        );
        (system, user)
    });

    let (tx, rx) = mpsc::channel::<String>(128);
    tokio::spawn(async move {
        let emit = |v: serde_json::Value| format!("data: {}\n\n", v);
        let _ = tx
            .send(emit(json!({
                "type": "meta",
                "mode": if llm.is_some() { "llm" } else { "retrieval" },
                "sources": sources,
                "contexts": contexts,
            })))
            .await;

        let Some(llm) = llm else {
            let _ = tx
                .send(emit(json!({
                    "type": "done",
                    "mode": "retrieval",
                    "answer": retrieval_answer,
                    "sources": sources,
                    "contexts": contexts,
                })))
                .await;
            return;
        };

        let (system, user) = prompt.expect("prompt built with llm");
        let tx_delta = tx.clone();
        let sink = Arc::new(move |s: &str| {
            if !s.is_empty() {
                let _ = tx_delta.try_send(emit(json!({ "type": "delta", "text": s })));
            }
        });

        match atlas_llm::with_stream_sink(sink, llm.chat(&system, &user)).await {
            Ok(resp) => {
                let _ = tx
                    .send(emit(json!({
                        "type": "done",
                        "mode": "llm",
                        "answer": resp.text,
                        "model": resp.model,
                        "sources": sources,
                        "contexts": contexts,
                        "usage": {
                            "prompt_tokens": resp.usage.prompt_tokens,
                            "completion_tokens": resp.usage.completion_tokens,
                        }
                    })))
                    .await;
            }
            Err(e) => {
                tracing::warn!("kb chat llm failed, falling back to retrieval: {e:#}");
                let _ = tx
                    .send(emit(json!({
                        "type": "done",
                        "mode": "retrieval",
                        "answer": retrieval_answer,
                        "sources": sources,
                        "contexts": contexts,
                    })))
                    .await;
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|chunk| Ok::<_, std::convert::Infallible>(chunk));
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
        "**未连接可用模型**，已改为直接返回知识库检索结果（问题：「{}」）。\n\n",
        question
    ));
    if hits.is_empty() {
        md.push_str("没有命中相关内容。可先运行 `atlas init` / `atlas update`，或换更短的关键词。\n");
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
