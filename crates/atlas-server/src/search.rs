use super::*;
use crate::common::{err, open_project_store, resolve_project};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SearchResponse {
    project: String,
    hits: Vec<SearchHit>,
}

pub(crate) fn search_mode_for(pref: &atlas_core::projects::ProjectRef) -> atlas_store::SearchMode {
    atlas_store::SearchMode::parse(&pref.cfg.kb.search)
}

pub(crate) async fn kb_search(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let project = q.get("project").map(|s| s.as_str());
    let qtext = q.get("q").cloned().unwrap_or_default();
    let limit: i64 = q
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let pref = match resolve_project(&state, project) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let store = match open_project_store(&pref) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    match store.search(&qtext, limit, search_mode_for(&pref)) {
        Ok(hits) => Json(SearchResponse {
            project: pref.id.clone(),
            hits,
        })
        .into_response(),
        Err(e) => err(e),
    }
}
