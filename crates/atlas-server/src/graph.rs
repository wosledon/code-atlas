use super::*;
use crate::common::{err, open_project_store, resolve_project};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct GraphPayload {
    project: String,
    nodes: Vec<serde_json::Value>,
    edges: Vec<serde_json::Value>,
}

fn project_from_query(state: &AppState, q: &std::collections::HashMap<String, String>) -> Result<atlas_core::projects::ProjectRef> {
    resolve_project(state, q.get("project").map(|s| s.as_str()))
}

pub(crate) async fn list_entities(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let pref = match project_from_query(&state, &q) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let kind = q.get("kind").map(|s| s.as_str());
    match open_project_store(&pref) {
        Ok(store) => match store.list_entities(kind, 500) {
            Ok(list) => Json(json!({"project": pref.id, "entities": list})).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

pub(crate) async fn graph_nodes(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let pref = match project_from_query(&state, &q) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let store = match open_project_store(&pref) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    let entities = match store.list_entities(None, 1000) {
        Ok(e) => e,
        Err(e) => return err(e),
    };
    let relations = match store.list_relations(5000) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    let nodes: Vec<serde_json::Value> = entities
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "kind": e.kind,
                "name": e.name,
                "canonical_key": e.canonical_key,
                "status": e.status,
            })
        })
        .collect();
    let edges: Vec<serde_json::Value> = relations
        .into_iter()
        .map(|(src, dst, rel)| json!({"source": src, "target": dst, "rel": rel}))
        .collect();
    Json(GraphPayload {
        project: pref.id.clone(),
        nodes,
        edges,
    })
    .into_response()
}

pub(crate) async fn neighborhood(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let pref = match project_from_query(&state, &q) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let store = match open_project_store(&pref) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    match store.neighborhood(&id, 200) {
        Ok(rows) => Json(json!({
            "project": pref.id,
            "relations": rows
                .into_iter()
                .map(|(src, dst, rel)| json!({"source": src, "target": dst, "rel": rel}))
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => err(e),
    }
}
