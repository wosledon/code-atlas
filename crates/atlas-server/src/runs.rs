use super::*;
use crate::auth::{authorized, deny};
use crate::common::{err, open_store};

pub(crate) async fn list_runs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    match open_store(&state) {
        Ok(store) => match store.list_runs(50) {
            Ok(runs) => Json(runs).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}


pub(crate) async fn trigger_update(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    let ctx = atlas_core::pipeline::PipelineCtx {
        repo_root: state.repo_root.clone(),
        cfg: (*state.cfg).clone(),
    };
    match atlas_core::pipeline::run_init_or_update(&ctx, "update", None).await {
        Ok(res) => Json(res).into_response(),
        Err(e) => err(e),
    }
}
