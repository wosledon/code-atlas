use super::*;
use atlas_core::projects::{ProjectRef, ProjectRegistry};

pub(crate) fn open_project_store(pref: &ProjectRef) -> Result<Store> {
    atlas_core::pipeline::open_store(&pref.root, &pref.cfg)
}

/// Resolve a project using the cached registry (mtime hot-reload + throttled discovery).
pub(crate) fn resolve_project(state: &AppState, id: Option<&str>) -> Result<ProjectRef> {
    let mut guard = state.projects.write().unwrap_or_else(|e| e.into_inner());
    guard.refresh_if_changed();
    guard.discover_throttled();
    guard.resolve(id)
}

/// Full refresh from disk (list / register paths that need an up-to-date view).
pub(crate) fn reload_registry(state: &AppState) -> ProjectRegistry {
    let mut reg = ProjectRegistry::load_with_discovery(&state.repo_root);
    reg.refresh_if_changed();
    if let Ok(mut guard) = state.projects.write() {
        *guard = reg.clone();
    }
    reg
}

pub(crate) fn err(e: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": format!("{e:#}")})),
    )
        .into_response()
}
