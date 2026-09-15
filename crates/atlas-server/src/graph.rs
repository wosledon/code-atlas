use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, open_store};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct GraphPayload {
    nodes: Vec<serde_json::Value>,
    edges: Vec<serde_json::Value>,
}


pub(crate) async fn list_entities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let kind = q.get("kind").map(|s| s.as_str());
    match open_store(&state) {
        Ok(store) => match store.list_entities(kind, 500) {
            Ok(list) => Json(list).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}


pub(crate) async fn graph_nodes(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let store = match open_store(&state) {
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
    Json(GraphPayload { nodes, edges }).into_response()
}


pub(crate) async fn neighborhood(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(id): AxPath<String>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let store = match open_store(&state) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    match store.neighborhood(&id, 200) {
        Ok(rows) => Json(rows
            .into_iter()
            .map(|(src, dst, rel)| json!({"source": src, "target": dst, "rel": rel}))
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => err(e),
    }
}
