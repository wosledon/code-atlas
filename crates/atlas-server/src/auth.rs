use super::*;

pub(crate) fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    if state.token.as_str() == "insecure" {
        return true;
    }
    if let Some(auth) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(b) = s.strip_prefix("Bearer ") {
                if b == state.token.as_str() {
                    return true;
                }
            }
        }
    }
    if let Some(c) = headers.get(axum::http::header::COOKIE) {
        if let Ok(s) = c.to_str() {
            let expected = format!("atlas_token={}", state.token.as_str());
            if s.split(';')
                .map(str::trim)
                .any(|kv| kv == expected || kv.starts_with(&format!("{expected};")))
            {
                return true;
            }
        }
    }
    false
}


pub(crate) fn deny() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response()
}
