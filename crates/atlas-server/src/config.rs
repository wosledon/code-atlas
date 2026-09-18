use super::*;
use crate::common::{err, resolve_project};
use axum::extract::Query as AxQuery;
use serde::Deserialize;

#[derive(Deserialize)]
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
    /// Target project; empty → launch project.
    #[serde(default)]
    project: Option<String>,
}

pub(crate) async fn get_config(
    State(state): State<AppState>,
    AxQuery(q): AxQuery<std::collections::HashMap<String, String>>,
) -> Response {
    let pref = match resolve_project(&state, q.get("project").map(|s| s.as_str())) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let cfg = &pref.cfg;
    // Never echo the key itself — only whether one is available.
    let has_cfg_key = !cfg.llm.api_key.trim().is_empty();
    let has_openai_key = has_cfg_key
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ATLAS_API_KEY").is_ok();
    let has_anthropic_key = has_cfg_key || std::env::var("ANTHROPIC_API_KEY").is_ok();
    Json(json!({
        "project": pref.id,
        "provider": cfg.llm.provider,
        "model": cfg.llm.model,
        "base_url": cfg.llm.base_url,
        "temperature": cfg.llm.temperature,
        "max_output_tokens": cfg.llm.max_output_tokens,
        "concurrency": cfg.llm.concurrency,
        "timeout_secs": cfg.llm.timeout_secs,
        "language": cfg.output.language,
        "strategy": cfg.output.strategy,
        "atlas_root": pref.atlas_root.display().to_string(),
        "chunk_mode": cfg.kb.chunk.mode,
        "target_tokens": cfg.kb.chunk.target_tokens,
        "has_openai_key": has_openai_key,
        "has_anthropic_key": has_anthropic_key,
    }))
    .into_response()
}

/// Rewrite `atlas.toml` (non-secret fields only) for the target project.
pub(crate) async fn post_config(
    State(state): State<AppState>,
    Json(body): Json<ConfigUpdate>,
) -> Response {
    let pref = match resolve_project(&state, body.project.as_deref()) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let path = pref.root.join("atlas.toml");
    let mut cfg = if path.exists() {
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<AtlasConfig>(&t).ok())
        {
            Some(c) => c,
            None => pref.cfg.clone(),
        }
    } else {
        pref.cfg.clone()
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
            if let Err(e) = std::fs::write(&path, &text) {
                return err(e.into());
            }
            // Refresh cached registry cfg view after writing.
            let _ = crate::common::reload_registry(&state);
            Json(json!({
                "ok": true,
                "project": pref.id,
                "path": path.display().to_string(),
            }))
            .into_response()
        }
        Err(e) => err(e.into()),
    }
}
