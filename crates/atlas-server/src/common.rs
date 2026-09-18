use super::*;
use atlas_core::projects::{ProjectRef, ProjectRegistry};

pub(crate) fn open_project_store(pref: &ProjectRef) -> Result<Store> {
    atlas_core::pipeline::open_store(&pref.root, &pref.cfg)
}

/// Resolve a project, **hot-reloading** the registry from disk on every call
/// so `atlas project add/remove` and external marker discovery apply without
/// restarting `atlas web`.
pub(crate) fn resolve_project(state: &AppState, id: Option<&str>) -> Result<ProjectRef> {
    let reg = reload_registry(state);
    reg.resolve(id)
}

/// Reload registry from disk and replace the in-memory cache (hot reload).
pub(crate) fn reload_registry(state: &AppState) -> ProjectRegistry {
    let reg = ProjectRegistry::load_with_discovery(&state.repo_root);
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
