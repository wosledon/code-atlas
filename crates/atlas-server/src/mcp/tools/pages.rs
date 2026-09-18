use super::*;
use anyhow::{bail, Result};
use atlas_core::pipeline;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) fn list_pages(atlas_root: &Path) -> Vec<String> {
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

pub(crate) fn read_page(atlas_root: &Path, rel: &str) -> Result<String> {
    let full = resolve_page(atlas_root, rel)?;
    Ok(std::fs::read_to_string(&full)?)
}

pub(crate) fn resolve_page(atlas_root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim().trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || Path::new(rel).is_absolute() {
        bail!("invalid page path: {rel}");
    }
    if Path::new(rel).extension().and_then(|e| e.to_str()) != Some("md") {
        bail!("only .md pages are allowed: {rel}");
    }
    let joined = atlas_root.join(rel);
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

pub(crate) fn write_page(
    pref: &ProjectRef,
    rel: &str,
    body: &str,
    page_type: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    reindex: bool,
) -> Result<String> {
    let full = resolve_page(&pref.atlas_root, rel)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (fm_text, content) = split_front_matter(body);
    let content = content.trim();
    let bytes = if let Some(fm) = fm_text {
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
        pipeline::reindex(&pref.root, &pref.cfg)?
    } else {
        0
    };
    let rel_norm = rel.trim().trim_start_matches('/').replace('\\', "/");
    Ok(json!({
        "ok": true,
        "project": pref.id,
        "path": rel_norm,
        "bytes": bytes.len(),
        "reindexed_pages": n,
    })
    .to_string())
}

pub(crate) fn delete_page(pref: &ProjectRef, rel: &str, reindex: bool) -> Result<String> {
    let full = resolve_page(&pref.atlas_root, rel)?;
    if !full.exists() {
        bail!("page not found: {rel}");
    }
    std::fs::remove_file(&full)?;
    let n = if reindex {
        pipeline::reindex(&pref.root, &pref.cfg)?
    } else {
        0
    };
    Ok(json!({
        "ok": true,
        "project": pref.id,
        "deleted": rel.trim().trim_start_matches('/'),
        "reindexed_pages": n,
    })
    .to_string())
}

pub(crate) fn patch_page(pref: &ProjectRef, rel: &str, args: &Value, reindex: bool) -> Result<String> {
    let full = resolve_page(&pref.atlas_root, rel)?;
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
        pipeline::reindex(&pref.root, &pref.cfg)?
    } else {
        0
    };
    Ok(json!({
        "ok": true,
        "project": pref.id,
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
