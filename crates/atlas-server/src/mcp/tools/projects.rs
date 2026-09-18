use super::*;
use anyhow::Result;
use serde_json::{json, Value};

pub(crate) fn list_projects_json(ctx: &McpCtx) -> Result<String> {
    let reg = list_registry(ctx);
    let items: Vec<Value> = reg
        .list()
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name,
                "root": p.root.display().to_string(),
                "atlas_root": p.atlas_root.display().to_string(),
                "provider": p.cfg.llm.provider,
                "model": p.cfg.llm.model,
                "is_launch": p.is_launch,
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&json!({
        "launch": reg.launch_id(),
        "registry_path": reg.registry_path().display().to_string(),
        "projects": items,
    }))?)
}

pub(crate) fn repo_info(ctx: &McpCtx, project: Option<&str>) -> Result<String> {
    let pref = resolve_ctx_project(ctx, project)?;
    let reg = list_registry(ctx);
    let store = atlas_core::pipeline::open_store(&pref.root, &pref.cfg).ok();
    let (pages, chunks, entities) = match &store {
        Some(s) => (
            s.count_pages().unwrap_or(0),
            s.count_chunks().unwrap_or(0),
            s.count_entities().unwrap_or(0),
        ),
        None => (0, 0, 0),
    };
    let on_disk = list_pages(&pref.atlas_root).len() as i64;
    Ok(serde_json::to_string_pretty(&json!({
        "project": pref.id,
        "is_launch": pref.is_launch,
        "repo_root": pref.root.display().to_string(),
        "atlas_root": pref.atlas_root.display().to_string(),
        "provider": pref.cfg.llm.provider,
        "model": pref.cfg.llm.model,
        "base_url": pref.cfg.llm.base_url,
        "language": pref.cfg.output.language,
        "strategy": pref.cfg.output.strategy,
        "has_api_key": !pref.cfg.llm.api_key.trim().is_empty(),
        "pages_on_disk": on_disk,
        "pages_indexed": pages,
        "chunks": chunks,
        "entities": entities,
        "projects": reg.list().into_iter().map(|p| p.id).collect::<Vec<_>>(),
    }))?)
}

