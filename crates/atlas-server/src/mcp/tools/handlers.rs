use super::*;
use super::pages::{delete_page, list_pages, patch_page, read_page, write_page};
use anyhow::{bail, Result};
use atlas_core::pipeline;
use atlas_store::SearchMode;
use serde_json::{json, Value};

pub(crate) async fn call_tool(ctx: &McpCtx, name: &str, args: &Value) -> Result<String> {
    let project = args.get("project").and_then(|v| v.as_str());
    match name {
        "atlas_search" => {
            let q = str_arg(args, "query")?;
            let limit = int_arg(args, "limit").unwrap_or(10).clamp(1, 50);
            let pref = resolve_ctx_project(ctx, project)?;
            let store = pipeline::open_store(&pref.root, &pref.cfg)?;
            let hits = store.search(q, limit, SearchMode::parse(&pref.cfg.kb.search))?;
            Ok(serde_json::to_string_pretty(&json!({
                "project": pref.id,
                "hits": hits,
            }))?)
        }
        "atlas_read_page" => {
            let pref = resolve_ctx_project(ctx, project)?;
            read_page(&pref.atlas_root, str_arg(args, "path")?)
        }
        "atlas_list_pages" => {
            let pref = resolve_ctx_project(ctx, project)?;
            Ok(serde_json::to_string_pretty(&json!({
                "project": pref.id,
                "pages": list_pages(&pref.atlas_root),
            }))?)
        }
        "atlas_get_chunks" => {
            let path = str_arg(args, "path")?;
            let pref = resolve_ctx_project(ctx, project)?;
            let store = pipeline::open_store(&pref.root, &pref.cfg)?;
            let chunks = store.list_chunks_for_page(path)?;
            if chunks.is_empty() {
                bail!("no chunks for `{path}` (run atlas_update / reindex first)");
            }
            Ok(serde_json::to_string_pretty(&chunks)?)
        }
        "atlas_status" => {
            let last = bool_arg(args, "last").unwrap_or(false);
            let pref = resolve_ctx_project(ctx, project)?;
            let store = pipeline::open_store(&pref.root, &pref.cfg)?;
            let runs = store.list_runs(if last { 1 } else { 10 })?;
            Ok(serde_json::to_string_pretty(&json!({
                "project": pref.id,
                "runs": runs,
            }))?)
        }
        "atlas_list_projects" => list_projects_json(ctx),
        "atlas_repo_info" => repo_info(ctx, project),
        "atlas_plan" => {
            let instruction = args.get("instruction").and_then(|v| v.as_str());
            let pref = resolve_ctx_project(ctx, project)?;
            let pages = pipeline::preview_plan(&pref.root, instruction)?;
            let lines: Vec<String> = pages
                .iter()
                .map(|p| format!("{}\t{}\t{}", p.rel_path, p.page_type, p.description))
                .collect();
            Ok(format!(
                "planned {} pages for project `{}`\n{}",
                pages.len(),
                pref.id,
                lines.join("\n")
            ))
        }
        "atlas_check" => {
            let pref = resolve_ctx_project(ctx, project)?;
            let issues = pipeline::run_check(&pref.root, &pref.cfg)?;
            if issues.is_empty() {
                Ok(format!("ok ({})", pref.id))
            } else {
                Ok(issues.join("\n"))
            }
        }
        "atlas_reindex" => {
            let pref = resolve_ctx_project(ctx, project)?;
            let n = pipeline::reindex(&pref.root, &pref.cfg)?;
            Ok(format!("reindexed {n} pages for `{}`", pref.id))
        }
        "atlas_update" => {
            let mode = str_arg(args, "mode").unwrap_or("update");
            if mode != "update" && mode != "init" {
                bail!("mode must be update or init");
            }
            let instruction = args.get("instruction").and_then(|v| v.as_str());
            let pref = resolve_ctx_project(ctx, project)?;
            let pctx = pipeline::PipelineCtx {
                repo_root: pref.root.clone(),
                cfg: pref.cfg.clone(),
            };
            match pipeline::run_init_or_update(&pctx, mode, instruction).await {
                Ok(res) => Ok(serde_json::to_string_pretty(&json!({
                    "project": pref.id,
                    "repo_root": pref.root.display().to_string(),
                    "result": res,
                }))?),
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg.contains("atlas lock held") {
                        bail!("project `{}` is busy: {msg}", pref.id);
                    }
                    Err(e)
                }
            }
        }
        "atlas_list_entities" => {
            let kind = args.get("kind").and_then(|v| v.as_str());
            let limit = int_arg(args, "limit").unwrap_or(50).clamp(1, 500);
            let pref = resolve_ctx_project(ctx, project)?;
            let store = pipeline::open_store(&pref.root, &pref.cfg)?;
            let list = store.list_entities(kind, limit)?;
            Ok(serde_json::to_string_pretty(&json!({
                "project": pref.id,
                "entities": list,
            }))?)
        }
        "atlas_write_page" => {
            let path = str_arg(args, "path")?;
            let body = str_arg(args, "body")?;
            let page_type = args.get("type").and_then(|v| v.as_str());
            let title = args.get("title").and_then(|v| v.as_str());
            let description = args.get("description").and_then(|v| v.as_str());
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            let pref = resolve_ctx_project(ctx, project)?;
            write_page(&pref, path, body, page_type, title, description, reindex)
        }
        "atlas_delete_page" => {
            let path = str_arg(args, "path")?;
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            let pref = resolve_ctx_project(ctx, project)?;
            delete_page(&pref, path, reindex)
        }
        "atlas_patch_page" => {
            let path = str_arg(args, "path")?;
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            let pref = resolve_ctx_project(ctx, project)?;
            patch_page(&pref, path, args, reindex)
        }
        "atlas_graph_neighborhood" => {
            let id = str_arg(args, "id")?;
            let limit = int_arg(args, "limit").unwrap_or(50).clamp(1, 200);
            let pref = resolve_ctx_project(ctx, project)?;
            let store = pipeline::open_store(&pref.root, &pref.cfg)?;
            let rels = store.neighborhood(id, limit)?;
            let rows: Vec<Value> = rels
                .into_iter()
                .map(|(src, dst, rel)| json!({"src": src, "dst": dst, "rel": rel}))
                .collect();
            Ok(serde_json::to_string_pretty(&json!({
                "project": pref.id,
                "relations": rows,
            }))?)
        }
        other => bail!("unknown tool: {other}"),
    }
}
