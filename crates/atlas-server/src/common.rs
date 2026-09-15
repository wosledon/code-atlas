use super::*;

pub(crate) fn open_store(state: &AppState) -> Result<Store> {
    atlas_core::pipeline::open_store(&state.repo_root, &state.cfg)
}


pub(crate) fn err(e: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": format!("{e:#}")})),
    )
        .into_response()
}
