use super::*;
use crate::common::{err, open_project_store, resolve_project};
use crate::projects::{run_project_update, UpdateProjectBody};
use axum::extract::Query as AxQuery;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct ProjectQuery {
    /// Project id; empty / omitted / `default` → launch project.
    #[serde(default)]
    project: Option<String>,
}

pub(crate) async fn list_runs(
    State(state): State<AppState>,
    AxQuery(q): AxQuery<ProjectQuery>,
) -> Response {
    let pref = match resolve_project(&state, q.project.as_deref()) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    match open_project_store(&pref) {
        Ok(store) => match store.list_runs(50) {
            Ok(runs) => Json(json!({
                "projectId": pref.id,
                "projectName": pref.name,
                "runs": runs,
            }))
            .into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

/// Legacy `POST /api/run/update` — still project-dimension.
/// Body may include `project` (default: launch project).
pub(crate) async fn trigger_update_with_project(
    State(state): State<AppState>,
    body: Option<Json<UpdateProjectBody>>,
) -> Response {
    let body = body.map(|Json(b)| b).unwrap_or_else(|| UpdateProjectBody {
        mode: "update".into(),
        instruction: None,
        project: None,
    });
    if body.mode != "update" && body.mode != "init" {
        return err(anyhow::anyhow!("mode must be update or init"));
    }
    let pref = match resolve_project(&state, body.project.as_deref()) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    run_project_update(&pref, &body.mode, body.instruction.as_deref()).await
}
