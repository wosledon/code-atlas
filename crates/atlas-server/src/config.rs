use super::*;
use crate::auth::{authorized, deny};
use crate::common::err;

#[derive(serde::Deserialize)]
pub(crate) struct ConfigUpdate {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    chunk_mode: Option<String>,
    #[serde(default)]
    strategy: Option<String>,
}


pub(crate) async fn get_config(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let cfg = &state.cfg;
    Json(json!({
        "provider": cfg.llm.provider,
        "model": cfg.llm.model,
        "base_url": cfg.llm.base_url,
        "temperature": cfg.llm.temperature,
        "max_output_tokens": cfg.llm.max_output_tokens,
        "concurrency": cfg.llm.concurrency,
        "timeout_secs": cfg.llm.timeout_secs,
        "language": cfg.output.language,
        "strategy": cfg.output.strategy,
        "atlas_root": state.atlas_root.display().to_string(),
        "chunk_mode": cfg.kb.chunk.mode,
        "target_tokens": cfg.kb.chunk.target_tokens,
        "has_openai_key": std::env::var("OPENAI_API_KEY").is_ok(),
        "has_anthropic_key": std::env::var("ANTHROPIC_API_KEY").is_ok(),
    }))
    .into_response()
}


pub(crate) async fn post_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ConfigUpdate>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    // Rewrite `atlas.toml` (non-secret fields only).
    let path = state.repo_root.join("atlas.toml");
    let mut cfg = if path.exists() {
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<AtlasConfig>(&t).ok())
        {
            Some(c) => c,
            None => (*state.cfg).clone(),
        }
    } else {
        (*state.cfg).clone()
    };
    if let Some(v) = body.provider {
        cfg.llm.provider = v;
    }
    if let Some(v) = body.model {
        cfg.llm.model = v;
    }
    if let Some(v) = body.base_url {
        cfg.llm.base_url = v;
    }
    if let Some(v) = body.temperature {
        cfg.llm.temperature = v;
    }
    if let Some(v) = body.language {
        cfg.output.language = v;
    }
    if let Some(v) = body.chunk_mode {
        cfg.kb.chunk.mode = v;
    }
    if let Some(v) = body.strategy {
        cfg.output.strategy = v;
    }
    match toml::to_string_pretty(&cfg) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                return err(e.into());
            }
            Json(json!({"ok": true, "path": path.display().to_string()})).into_response()
        }
        Err(e) => err(e.into()),
    }
}
