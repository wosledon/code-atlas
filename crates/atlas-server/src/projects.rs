use super::*;
use crate::common::{err, open_project_store, reload_registry, resolve_project};
use atlas_core::projects::ProjectRef;
use serde::{Deserialize, Serialize};

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
    #[serde(rename = "isLaunch")]
    is_launch: bool,
    highlights: Vec<ProjectEntry>,
}

#[derive(Serialize)]
pub(crate) struct ProjectsResponse {
    projects: Vec<Project>,
}

#[derive(Deserialize)]
pub(crate) struct RegisterProjectBody {
    root: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// Body for project-scoped update. Shared by `/api/projects/{id}/update`
/// and legacy `/api/run/update`.
#[derive(Deserialize)]
pub(crate) struct UpdateProjectBody {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub instruction: Option<String>,
    /// Optional project id (used by the legacy `/api/run/update` body).
    #[serde(default)]
    pub project: Option<String>,
}

impl Default for UpdateProjectBody {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            instruction: None,
            project: None,
        }
    }
}

fn default_mode() -> String {
    "update".into()
}

/// Landing page data: one card per project in the registry (launch repo first).
/// Registry is hot-reloaded from disk on every list.
pub(crate) async fn list_projects(State(state): State<AppState>) -> Response {
    let list = reload_registry(&state).list();
    let mut projects = Vec::with_capacity(list.len());
    for pref in &list {
        match build_project_card(pref) {
            Ok(card) => projects.push(card),
            Err(e) => tracing::warn!("project `{}` card skipped: {e:#}", pref.id),
        }
    }
    Json(ProjectsResponse { projects }).into_response()
}

pub(crate) async fn register_project(
    State(state): State<AppState>,
    Json(body): Json<RegisterProjectBody>,
) -> Response {
    let root = std::path::PathBuf::from(&body.root);
    let mut guard = state
        .projects
        .write()
        .unwrap_or_else(|e| e.into_inner());
    match guard.register(&root, body.id.as_deref(), body.name.as_deref()) {
        Ok(pref) => match build_project_card(&pref) {
            Ok(card) => Json(card).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

pub(crate) async fn unregister_project(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Response {
    let mut guard = state
        .projects
        .write()
        .unwrap_or_else(|e| e.into_inner());
    match guard.unregister(&id) {
        Ok(()) => Json(json!({"ok": true, "id": id})).into_response(),
        Err(e) => err(e),
    }
}

/// Project-scoped doc update. `id` is the project id from `/api/projects`.
pub(crate) async fn trigger_project_update(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    body: Option<Json<UpdateProjectBody>>,
) -> Response {
    let mode = body.as_ref().map(|Json(b)| b.mode.clone()).unwrap_or_else(default_mode);
    let instruction = body.and_then(|Json(b)| b.instruction);
    if mode != "update" && mode != "init" {
        return err(anyhow::anyhow!("mode must be update or init"));
    }
    let pref = match resolve_project(&state, Some(&id)) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    run_project_update(&pref, &mode, instruction.as_deref()).await
}

pub(crate) async fn run_project_update(
    pref: &ProjectRef,
    mode: &str,
    instruction: Option<&str>,
) -> Response {
    let ctx = atlas_core::pipeline::PipelineCtx {
        repo_root: pref.root.clone(),
        cfg: pref.cfg.clone(),
    };
    match atlas_core::pipeline::run_init_or_update(&ctx, mode, instruction).await {
        Ok(res) => Json(json!({
            "project": pref.id,
            "repo_root": pref.root.display().to_string(),
            "run_id": res.run_id,
            "mode": res.mode,
            "status": res.status,
            "no_op": res.no_op,
            "pages_written": res.pages_written,
            "entities": res.entities,
            "chunks": res.chunks,
            "prompt_tokens": res.prompt_tokens,
            "completion_tokens": res.completion_tokens,
            "notes": res.notes,
        }))
        .into_response(),
        Err(e) => {
            // Per-project lock contention is expected when two UI actions race.
            let msg = format!("{e:#}");
            if msg.contains("atlas lock held") {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": msg,
                        "project": pref.id,
                        "hint": "another update is already running for this project",
                    })),
                )
                    .into_response();
            }
            err(e)
        }
    }
}

fn build_project_card(pref: &ProjectRef) -> anyhow::Result<Project> {
    let store = open_project_store(pref).ok();
    let runs = store
        .as_ref()
        .and_then(|s| s.list_runs(1).ok())
        .unwrap_or_default();
    let last = runs.first();
    let pages = store.as_ref().and_then(|s| s.count_pages().ok()).unwrap_or(0);
    let chunks = store.as_ref().and_then(|s| s.count_chunks().ok()).unwrap_or(0);
    let entities = store.as_ref().and_then(|s| s.count_entities().ok()).unwrap_or(0);
    let run_count = store
        .as_ref()
        .and_then(|s| s.count_runs().ok())
        .unwrap_or(0) as usize;

    let entry_path = if pref.atlas_root.join("quickstart.md").exists() {
        "quickstart.md".to_string()
    } else if pref.atlas_root.join("README.md").exists() {
        "README.md".to_string()
    } else {
        String::new()
    };

    let mut highlights: Vec<ProjectEntry> = Vec::new();
    if let Some(store) = store.as_ref() {
        for want in [
            "Architecture",
            "Data Model",
            "API Reference",
            "Business",
            "Workflow",
        ] {
            if highlights.len() >= 4 {
                break;
            }
            let found = store.list_pages().unwrap_or_default().into_iter().find(|p| {
                p.page_type == want
                    && !highlights.iter().any(|h| h.path == p.path)
                    && pref.atlas_root.join(&p.path).is_file()
            });
            if let Some(p) = found {
                highlights.push(ProjectEntry {
                    title: p.title,
                    path: p.path,
                });
            }
        }
    }

    let updated_at = std::fs::read_to_string(pref.atlas_root.join(".last-update.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v.get("updatedAt")
                .and_then(|x| x.as_str())
                .map(str::to_string)
        })
        .or_else(|| last.map(|r| r.created_at.clone()));

    Ok(Project {
        id: pref.id.clone(),
        name: pref.name.clone(),
        root: pref.root.display().to_string(),
        language: pref.cfg.output.language.clone(),
        provider: pref.cfg.llm.provider.clone(),
        model: pref.cfg.llm.model.clone(),
        pages,
        chunks,
        entities,
        runs: run_count,
        git_head: last.and_then(|r| r.git_head.clone()),
        updated_at,
        last_status: last.map(|r| r.status.clone()),
        entry_path,
        is_launch: pref.is_launch,
        highlights,
    })
}
