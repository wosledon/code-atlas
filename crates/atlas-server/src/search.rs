use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, open_store};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SearchResponse {
    hits: Vec<SearchHit>,
}


pub(crate) fn search_mode(state: &AppState) -> atlas_store::SearchMode {
    atlas_store::SearchMode::parse(&state.cfg.kb.search)
}


pub(crate) async fn kb_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let qtext = q.get("q").cloned().unwrap_or_default();
    let limit: i64 = q
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let store = match open_store(&state) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    match store.search(&qtext, limit, search_mode(&state)) {
        Ok(hits) => Json(SearchResponse { hits }).into_response(),
        Err(e) => err(e),
    }
}
