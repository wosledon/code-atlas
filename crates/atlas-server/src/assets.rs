use super::*;
use crate::auth::{authorized, deny};

pub(crate) async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    Json(json!({
        "ok": true,
        "atlas_root": state.atlas_root.display().to_string(),
        "language": state.cfg.output.language,
        "provider": state.cfg.llm.provider,
        "model": state.cfg.llm.model,
    }))
    .into_response()
}
