use super::*;
use crate::common::{err, resolve_project};
use axum::extract::Query as AxQuery;

/// Health for the active project (launch project when `project` is omitted).
pub(crate) async fn health(
    State(state): State<AppState>,
    AxQuery(q): AxQuery<std::collections::HashMap<String, String>>,
) -> Response {
    let pref = match resolve_project(&state, q.get("project").map(|s| s.as_str())) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    Json(json!({
        "ok": true,
        "project": pref.id,
        "is_launch": pref.is_launch,
        "atlas_root": pref.atlas_root.display().to_string(),
        "language": pref.cfg.output.language,
        "provider": pref.cfg.llm.provider,
        "model": pref.cfg.llm.model,
        "registry_path": state
            .projects
            .read()
            .map(|g| g.registry_path().display().to_string())
            .unwrap_or_default(),
    }))
    .into_response()
}
