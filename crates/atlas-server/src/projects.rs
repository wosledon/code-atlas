use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, open_store};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ProjectEntry {
    title: String,
    path: String,
}


#[derive(Serialize)]
pub(crate) struct Project {
    id: String,
    name: String,
    root: String,
    language: String,
    provider: String,
    model: String,
    pages: i64,
    chunks: i64,
    entities: i64,
    runs: usize,
    #[serde(rename = "gitHead")]
    git_head: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(rename = "lastStatus")]
    last_status: Option<String>,
    #[serde(rename = "entryPath")]
    entry_path: String,
    highlights: Vec<ProjectEntry>,
}


#[derive(Serialize)]
pub(crate) struct ProjectsResponse {
    projects: Vec<Project>,
}


/// Landing page data: one card per wiki (currently the repo this server runs for).
pub(crate) async fn list_projects(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let store = match open_store(&state) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    let runs = store.list_runs(1).unwrap_or_default();
    let last = runs.first();
    let pages = store.count_pages().unwrap_or_default();
    if pages == 0 {
        return Json(ProjectsResponse { projects: vec![] }).into_response();
    }
    let entry_path = if state.atlas_root.join("quickstart.md").exists() {
        "quickstart.md".to_string()
    } else if state.atlas_root.join("README.md").exists() {
        "README.md".to_string()
    } else {
        String::new()
    };
    let mut highlights: Vec<ProjectEntry> = Vec::new();
    // Highlights are picked by page type, not by hard-coded paths, so they work
    // for any output language or doc layout.
    for want in ["Architecture", "Data Model", "API Reference", "Business", "Workflow"] {
        if highlights.len() >= 4 {
            break;
        }
        let found = store
            .list_pages()
            .unwrap_or_default()
            .into_iter()
            .find(|p| {
                p.page_type == want
                    && !highlights.iter().any(|h| h.path == p.path)
                    && state.atlas_root.join(&p.path).is_file()
            });
        if let Some(p) = found {
            highlights.push(ProjectEntry {
                title: p.title,
                path: p.path,
            });
        }
    }
    let name = state
        .repo_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".into());
    let updated_at = std::fs::read_to_string(state.atlas_root.join(".last-update.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("updatedAt").and_then(|x| x.as_str()).map(str::to_string))
        .or_else(|| last.map(|r| r.created_at.clone()));
    Json(ProjectsResponse {
        projects: vec![Project {
            id: "default".into(),
            name,
            root: state.repo_root.display().to_string(),
            language: state.cfg.output.language.clone(),
            provider: state.cfg.llm.provider.clone(),
            model: state.cfg.llm.model.clone(),
            pages,
            chunks: store.count_chunks().unwrap_or_default(),
            entities: store.count_entities().unwrap_or_default(),
            runs: store.count_runs().unwrap_or_default() as usize,
            git_head: last.and_then(|r| r.git_head.clone()),
            updated_at,
            last_status: last.map(|r| r.status.clone()),
            entry_path,
            highlights,
        }],
    })
    .into_response()
}
