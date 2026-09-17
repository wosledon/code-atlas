use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, open_store};
use crate::search::search_mode;

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


pub(crate) async fn kb_chat(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<ChatRequest>) -> Response {
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
        Err(e) => return err(e),
    };
    let top_k = body.top_k.unwrap_or(8);
    let hits = match store.search(&user_msg, top_k, search_mode(&state)) {
        Ok(h) => h,
        Err(e) => return err(e),
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

    // Recalled material, shown in the UI next to the answer: the chunk summary
    // plus the chunk body itself (self-contained evidence, no extra disk reads).
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

    // no usable LLM → pure retrieval answer
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
        Ok(c) if c.configured() && !c.is_host_agent() => c,
        _ => {
            return Json(json!({
                "answer": retrieval_answer,
                "mode": "retrieval",
                "sources": sources,
                "contexts": contexts,
            }))
            .into_response();
        }
    };

    // assemble grounded context from the recalled chunks themselves
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

    let history: Vec<serde_json::Value> = body
        .messages
        .iter()
        .take(body.messages.len().saturating_sub(1))
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let system = format!(
        "You are Code Atlas assistant. Answer in {}. \
         Use ONLY the provided repository/wiki context for factual claims. \
         Cite page paths like `atlas/architecture/overview.md` when used. \
         If context is insufficient, say so briefly.\n\n## Context\n{}",
        state.cfg.output.language,
        context
    );

    let mut user = String::new();
    if !history.is_empty() {
        user.push_str("Conversation so far:\n");
        for h in &history {
            if let (Some(r), Some(c)) = (h["role"].as_str(), h["content"].as_str()) {
                user.push_str(&format!("{r}: {c}\n"));
            }
        }
        user.push('\n');
    }
    user.push_str(&format!("Question:\n{user_msg}"));

    match llm.chat(&system, &user).await {
        Ok(resp) => Json(json!({
            "answer": resp.text,
            "mode": "llm",
            "sources": sources,
            "contexts": contexts,
            "model": resp.model,
            "usage": {
                "prompt_tokens": resp.usage.prompt_tokens,
                "completion_tokens": resp.usage.completion_tokens,
            }
        }))
        .into_response(),
        Err(_) => Json(json!({
            "answer": retrieval_answer,
            "mode": "retrieval",
            "sources": sources,
            "contexts": contexts,
        }))
        .into_response(),
    }
}


pub(crate) fn format_hits_as_answer(question: &str, hits: &[SearchHit]) -> String {
    let mut md = String::new();
    md.push_str(&format!("**未连接可用模型**，已改为直接返回知识库检索结果（问题：「{}」）。\n\n", question));
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
