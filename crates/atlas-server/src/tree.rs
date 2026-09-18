use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, resolve_project};
use serde::Serialize;

pub(crate) async fn list_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let pref = match resolve_project(&state, q.get("project").map(|s| s.as_str())) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let mut pages = Vec::new();
    if pref.atlas_root.exists() {
        for entry in walkdir::WalkDir::new(&pref.atlas_root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("md") {
                let rel = entry
                    .path()
                    .strip_prefix(&pref.atlas_root)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");
                if !rel.starts_with('.') {
                    pages.push(rel);
                }
            }
        }
    }
    pages.sort();
    Json(json!({"project": pref.id, "pages": pages})).into_response()
}

#[derive(Serialize)]
pub(crate) struct TreeNode {
    id: String,
    name: String,
    path: Option<String>,
    children: Vec<TreeNode>,
}

pub(crate) async fn doc_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let pref = match resolve_project(&state, q.get("project").map(|s| s.as_str())) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let mut root = TreeNode {
        id: "root".into(),
        name: pref.name.clone(),
        path: None,
        children: vec![],
    };
    if !pref.atlas_root.exists() {
        return Json(json!({"project": pref.id, "tree": root})).into_response();
    }
    let mut files: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(&pref.atlas_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = entry
                .path()
                .strip_prefix(&pref.atlas_root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with('.') {
                continue;
            }
            files.push(rel);
        }
    }
    files.sort();
    for rel in files {
        let parts: Vec<&str> = rel.split('/').collect();
        let mut cur = &mut root;
        for (i, part) in parts.iter().enumerate() {
            let is_file = i == parts.len() - 1;
            let id = parts[..=i].join("/");
            if is_file {
                cur.children.push(TreeNode {
                    id: id.clone(),
                    name: part.to_string(),
                    path: Some(rel.clone()),
                    children: vec![],
                });
            } else {
                let idx = cur
                    .children
                    .iter()
                    .position(|c| c.name == *part && c.path.is_none());
                let idx = match idx {
                    Some(i) => i,
                    None => {
                        cur.children.push(TreeNode {
                            id: id.clone(),
                            name: part.to_string(),
                            path: None,
                            children: vec![],
                        });
                        cur.children.len() - 1
                    }
                };
                cur = &mut cur.children[idx];
            }
        }
    }
    Json(json!({"project": pref.id, "tree": root})).into_response()
}

pub(crate) async fn read_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(path): AxPath<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let pref = match resolve_project(&state, q.get("project").map(|s| s.as_str())) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let Some(full) = resolve_under_root(&pref.atlas_root, &path) else {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    };
    match std::fs::read_to_string(&full) {
        Ok(text) => Json(json!({
            "project": pref.id,
            "path": path,
            "content": text,
        }))
        .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Join `rel` onto `root` and verify the result really lives inside `root`.
pub(crate) fn resolve_under_root(root: &std::path::Path, rel: &str) -> Option<PathBuf> {
    if rel.trim().is_empty() {
        return None;
    }
    let candidate = root.join(rel);
    let root_real = root.canonicalize().ok()?;
    let full = candidate.canonicalize().ok()?;
    if !full.starts_with(&root_real) || !full.is_file() {
        return None;
    }
    Some(full)
}
