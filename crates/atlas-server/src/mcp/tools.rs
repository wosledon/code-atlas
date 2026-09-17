//! MCP tool catalogue and handlers for document maintenance.

use anyhow::{anyhow, bail, Result};
use atlas_core::{pipeline, AtlasConfig};
use atlas_store::SearchMode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) struct McpCtx {
    pub(crate) repo_root: PathBuf,
    pub(crate) cfg: AtlasConfig,
    pub(crate) atlas_root: PathBuf,
}

pub(crate) fn tool_defs() -> Value {
    json!([
        tool("atlas_search", "Search the knowledge base. Hits include page_path, line range and recalled body text.", json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "default 10"}
            },
            "required": ["query"]
        })),
        tool("atlas_read_page", "Read one generated wiki markdown page (path under atlas root).", json!({
            "type": "object",
            "properties": {"path": {"type": "string", "description": "e.g. quickstart.md"}},
            "required": ["path"]
        })),
        tool("atlas_list_pages", "List generated wiki markdown pages.", json!({"type": "object", "properties": {}})),
        tool("atlas_get_chunks", "Return KB chunks indexed for one page (ord, lines, body).", json!({
            "type": "object",
            "properties": {"path": {"type": "string", "description": "page path, e.g. quickstart.md"}},
            "required": ["path"]
        })),
        tool("atlas_status", "Show recent generation runs.", json!({
            "type": "object",
            "properties": {"last": {"type": "boolean", "description": "only the most recent run"}}
        })),
        tool("atlas_repo_info", "Repo / wiki overview: roots, provider, language, page and entity counts.", json!({"type": "object", "properties": {}})),
        tool("atlas_plan", "Preview the documentation page plan without calling an LLM.", json!({
            "type": "object",
            "properties": {"instruction": {"type": "string"}}
        })),
        tool("atlas_check", "Read-only integrity check (entry pages, link targets).", json!({"type": "object", "properties": {}})),
        tool("atlas_reindex", "Rebuild the SQLite index from markdown on disk.", json!({"type": "object", "properties": {}})),
        tool("atlas_update", "Run init/update pipeline (may call the LLM; takes minutes).", json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["update", "init"], "description": "default update"},
                "instruction": {"type": "string"}
            }
        })),
        tool("atlas_write_page", "Create or overwrite one wiki markdown page under the atlas root. Body may include YAML front matter (`---` ... `---`); if omitted, type/title/description arguments are used. Optionally reindexes the KB.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "relative path under atlas root, e.g. 03-模块详解/foo.md"},
                "body": {"type": "string", "description": "markdown body (with or without front matter)"},
                "type": {"type": "string", "description": "page type when body has no front matter, e.g. Module"},
                "title": {"type": "string"},
                "description": {"type": "string"},
                "reindex": {"type": "boolean", "description": "rebuild KB index after write (default true)"}
            },
            "required": ["path", "body"]
        })),
        tool("atlas_delete_page", "Delete one wiki markdown page under the atlas root and reindex.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "reindex": {"type": "boolean", "description": "default true"}
            },
            "required": ["path"]
        })),
        tool("atlas_patch_page", "Line- or text-level edit of one wiki page. Either find/replace, or replace a 1-based inclusive line range with text.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "find": {"type": "string", "description": "literal string to find"},
                "replace": {"type": "string", "description": "replacement for find"},
                "replace_all": {"type": "boolean", "description": "replace every occurrence (default false = first only)"},
                "start_line": {"type": "integer", "description": "1-based first line to replace"},
                "end_line": {"type": "integer", "description": "1-based last line to replace (inclusive)"},
                "text": {"type": "string", "description": "new text for the line range (may contain newlines)"},
                "reindex": {"type": "boolean", "description": "default true"}
            },
            "required": ["path"]
        })),
        tool("atlas_list_entities", "List knowledge-graph entities, optionally filtered by kind.", json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string"},
                "limit": {"type": "integer", "default": 50}
            }
        })),
        tool("atlas_graph_neighborhood", "Relations touching one entity id (src/dst/rel triples).", json!({
            "type": "object",
            "properties": {"id": {"type": "string"}, "limit": {"type": "integer", "default": 50}},
            "required": ["id"]
        }))
    ])
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

pub(crate) async fn call_tool(ctx: &McpCtx, name: &str, args: &Value) -> Result<String> {
    match name {
        "atlas_search" => {
            let q = str_arg(args, "query")?;
            let limit = int_arg(args, "limit").unwrap_or(10).clamp(1, 50);
            let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg)?;
            let hits = store.search(q, limit, SearchMode::parse(&ctx.cfg.kb.search))?;
            Ok(serde_json::to_string_pretty(&hits)?)
        }
        "atlas_read_page" => read_page(&ctx.atlas_root, str_arg(args, "path")?),
        "atlas_list_pages" => Ok(serde_json::to_string_pretty(&list_pages(&ctx.atlas_root))?),
        "atlas_get_chunks" => {
            let path = str_arg(args, "path")?;
            let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg)?;
            let chunks = store.list_chunks_for_page(path)?;
            if chunks.is_empty() {
                bail!("no chunks for `{path}` (run atlas_update / reindex first)");
            }
            Ok(serde_json::to_string_pretty(&chunks)?)
        }
        "atlas_status" => {
            let last = bool_arg(args, "last").unwrap_or(false);
            let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg)?;
            let runs = store.list_runs(if last { 1 } else { 10 })?;
            Ok(serde_json::to_string_pretty(&runs)?)
        }
        "atlas_repo_info" => repo_info(ctx),
        "atlas_plan" => {
            let instruction = args.get("instruction").and_then(|v| v.as_str());
            let pages = pipeline::preview_plan(&ctx.repo_root, instruction)?;
            let lines: Vec<String> = pages
                .iter()
                .map(|p| format!("{}\t{}\t{}", p.rel_path, p.page_type, p.description))
                .collect();
            Ok(format!("planned {} pages\n{}", pages.len(), lines.join("\n")))
        }
        "atlas_check" => {
            let issues = pipeline::run_check(&ctx.repo_root, &ctx.cfg)?;
            if issues.is_empty() {
                Ok("ok".into())
            } else {
                Ok(issues.join("\n"))
            }
        }
        "atlas_reindex" => {
            let n = pipeline::reindex(&ctx.repo_root, &ctx.cfg)?;
            Ok(format!("reindexed {n} pages"))
        }
        "atlas_update" => {
            let mode = str_arg(args, "mode").unwrap_or("update");
            if mode != "update" && mode != "init" {
                bail!("mode must be update or init");
            }
            let instruction = args.get("instruction").and_then(|v| v.as_str());
            let pctx = pipeline::PipelineCtx {
                repo_root: ctx.repo_root.clone(),
                cfg: ctx.cfg.clone(),
            };
            let res = pipeline::run_init_or_update(&pctx, mode, instruction).await?;
            Ok(serde_json::to_string_pretty(&res)?)
        }
        "atlas_list_entities" => {
            let kind = args.get("kind").and_then(|v| v.as_str());
            let limit = int_arg(args, "limit").unwrap_or(50).clamp(1, 500);
            let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg)?;
            let list = store.list_entities(kind, limit)?;
            Ok(serde_json::to_string_pretty(&list)?)
        }
        "atlas_write_page" => {
            let path = str_arg(args, "path")?;
            let body = str_arg(args, "body")?;
            let page_type = args.get("type").and_then(|v| v.as_str());
            let title = args.get("title").and_then(|v| v.as_str());
            let description = args.get("description").and_then(|v| v.as_str());
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            write_page(ctx, path, body, page_type, title, description, reindex)
        }
        "atlas_delete_page" => {
            let path = str_arg(args, "path")?;
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            delete_page(ctx, path, reindex)
        }
        "atlas_patch_page" => {
            let path = str_arg(args, "path")?;
            let reindex = bool_arg(args, "reindex").unwrap_or(true);
            patch_page(ctx, path, args, reindex)
        }
        "atlas_graph_neighborhood" => {
            let id = str_arg(args, "id")?;
            let limit = int_arg(args, "limit").unwrap_or(50).clamp(1, 200);
            let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg)?;
            let rels = store.neighborhood(id, limit)?;
            let rows: Vec<Value> = rels
                .into_iter()
                .map(|(src, dst, rel)| json!({"src": src, "dst": dst, "rel": rel}))
                .collect();
            Ok(serde_json::to_string_pretty(&rows)?)
        }
        other => bail!("unknown tool: {other}"),
    }
}

fn repo_info(ctx: &McpCtx) -> Result<String> {
    let store = pipeline::open_store(&ctx.repo_root, &ctx.cfg).ok();
    let (pages, chunks, entities) = match &store {
        Some(s) => (
            s.count_pages().unwrap_or(0),
            s.count_chunks().unwrap_or(0),
            s.count_entities().unwrap_or(0),
        ),
        None => (0, 0, 0),
    };
    let on_disk = list_pages(&ctx.atlas_root).len() as i64;
    Ok(serde_json::to_string_pretty(&json!({
        "repo_root": ctx.repo_root.display().to_string(),
        "atlas_root": ctx.atlas_root.display().to_string(),
        "provider": ctx.cfg.llm.provider,
        "model": ctx.cfg.llm.model,
        "base_url": ctx.cfg.llm.base_url,
        "language": ctx.cfg.output.language,
        "strategy": ctx.cfg.output.strategy,
        "has_api_key": !ctx.cfg.llm.api_key.trim().is_empty(),
        "pages_on_disk": on_disk,
        "pages_indexed": pages,
        "chunks": chunks,
        "entities": entities,
    }))?)
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing argument `{key}`"))
}

fn int_arg(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

fn bool_arg(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn list_pages(atlas_root: &Path) -> Vec<String> {
    let mut pages = Vec::new();
    if !atlas_root.exists() {
        return pages;
    }
    for entry in walkdir::WalkDir::new(atlas_root).into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = entry
                .path()
                .strip_prefix(atlas_root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if !rel.starts_with('.') {
                pages.push(rel);
            }
        }
    }
    pages.sort();
    pages
}

fn read_page(atlas_root: &Path, rel: &str) -> Result<String> {
    let full = resolve_page(atlas_root, rel)?;
    Ok(std::fs::read_to_string(&full)?)
}

fn resolve_page(atlas_root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim().trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || Path::new(rel).is_absolute() {
        bail!("invalid page path: {rel}");
    }
    if Path::new(rel).extension().and_then(|e| e.to_str()) != Some("md") {
        bail!("only .md pages are allowed: {rel}");
    }
    let joined = atlas_root.join(rel);
    // Parent may not exist yet on write; check the deepest existing ancestor.
    if let (Ok(root_c), Ok(existing)) = (
        atlas_root.canonicalize(),
        nearest_existing(&joined).canonicalize(),
    ) {
        if !existing.starts_with(&root_c) {
            bail!("page outside atlas root: {rel}");
        }
    }
    Ok(joined)
}

fn nearest_existing(path: &Path) -> &Path {
    let mut cur = path;
    while !cur.exists() {
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    cur
}

fn write_page(
    ctx: &McpCtx,
    rel: &str,
    body: &str,
    page_type: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    reindex: bool,
) -> Result<String> {
    let full = resolve_page(&ctx.atlas_root, rel)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (fm_text, content) = split_front_matter(body);
    let content = content.trim();
    let bytes = if let Some(fm) = fm_text {
        // Keep author-supplied front matter; normalize trailing newline.
        format!("---\n{fm}---\n{content}\n").into_bytes()
    } else {
        let title = title
            .map(str::to_string)
            .unwrap_or_else(|| first_heading(content).unwrap_or_else(|| "Untitled".into()));
        let ptype = page_type.unwrap_or("Note").to_string();
        let desc = description.unwrap_or("").to_string();
        let fm = atlas_core::markdown::FrontMatter::new(&ptype, &title, &desc, &[]);
        format!("{}{content}\n", fm.render()).into_bytes()
    };
    atlas_core::markdown::atomic_write(&full, &bytes)?;
    let n = if reindex {
        pipeline::reindex(&ctx.repo_root, &ctx.cfg)?
    } else {
        0
    };
    let rel_norm = rel.trim().trim_start_matches('/').replace('\\', "/");
    Ok(json!({
        "ok": true,
        "path": rel_norm,
        "bytes": bytes.len(),
        "reindexed_pages": n,
    })
    .to_string())
}

fn delete_page(ctx: &McpCtx, rel: &str, reindex: bool) -> Result<String> {
    let full = resolve_page(&ctx.atlas_root, rel)?;
    if !full.exists() {
        bail!("page not found: {rel}");
    }
    std::fs::remove_file(&full)?;
    let n = if reindex {
        pipeline::reindex(&ctx.repo_root, &ctx.cfg)?
    } else {
        0
    };
    Ok(json!({"ok": true, "deleted": rel.trim().trim_start_matches('/'), "reindexed_pages": n}).to_string())
}

fn patch_page(ctx: &McpCtx, rel: &str, args: &Value, reindex: bool) -> Result<String> {
    let full = resolve_page(&ctx.atlas_root, rel)?;
    if !full.exists() {
        bail!("page not found: {rel}");
    }
    let original = std::fs::read_to_string(&full)?;
    let find = args.get("find").and_then(|v| v.as_str());
    let replace = args.get("replace").and_then(|v| v.as_str()).unwrap_or("");
    let start_line = int_arg(args, "start_line");
    let end_line = int_arg(args, "end_line");
    let text = args.get("text").and_then(|v| v.as_str());

    let (patched, note) = if let Some(needle) = find.filter(|s| !s.is_empty()) {
        if needle.is_empty() {
            bail!("find must not be empty");
        }
        let replace_all = bool_arg(args, "replace_all").unwrap_or(false);
        let count = original.matches(needle).count();
        if count == 0 {
            bail!("find string not found in `{rel}`");
        }
        let patched = if replace_all {
            original.replace(needle, replace)
        } else {
            original.replacen(needle, replace, 1)
        };
        let n = if replace_all { count } else { 1 };
        (patched, format!("replaced {n} occurrence(s)"))
    } else if let (Some(start), Some(end)) = (start_line, end_line) {
        let Some(new_text) = text else {
            bail!("line-range patch requires `text`");
        };
        let lines: Vec<&str> = original.split_inclusive('\n').collect();
        let total = lines.len() as i64;
        if start < 1 || end < start || start > total {
            bail!("invalid line range {start}-{end} (file has {total} lines)");
        }
        let end = end.min(total);
        let mut out = String::new();
        out.push_str(&lines[..(start - 1) as usize].concat());
        out.push_str(new_text);
        if !new_text.ends_with('\n') && (end as usize) < lines.len() {
            out.push('\n');
        }
        // Keep remainder after the replaced range.
        if (end as usize) < lines.len() {
            out.push_str(&lines[end as usize..].concat());
        } else if !out.ends_with('\n') {
            out.push('\n');
        }
        (out, format!("replaced lines {start}-{end}"))
    } else {
        bail!("provide either find/replace, or start_line+end_line+text");
    };

    atlas_core::markdown::atomic_write(&full, patched.as_bytes())?;
    let n = if reindex {
        pipeline::reindex(&ctx.repo_root, &ctx.cfg)?
    } else {
        0
    };
    Ok(json!({
        "ok": true,
        "path": rel.trim().trim_start_matches('/'),
        "note": note,
        "bytes_before": original.len(),
        "bytes_after": patched.len(),
        "reindexed_pages": n,
    })
    .to_string())
}

fn split_front_matter(body: &str) -> (Option<String>, &str) {
    let trimmed = body.trim_start_matches('\n');
    if !trimmed.starts_with("---") {
        return (None, trimmed);
    }
    let rest = &trimmed[3..];
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    if let Some(end) = rest.find("\n---") {
        let fm = &rest[..end];
        let after = &rest[end + 4..];
        let after = after.strip_prefix('\n').unwrap_or(after);
        return (Some(format!("{fm}\n")), after);
    }
    (None, trimmed)
}

fn first_heading(md: &str) -> Option<String> {
    for line in md.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix('#') {
            let title = h.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}
