//! HTTP oneshot tests for project-dimension API contracts.

use atlas_core::projects::ProjectRegistry;
use atlas_core::AtlasConfig;
use atlas_server::{build_api_router, build_state_with_registry};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use std::fs;
use std::path::PathBuf;
use tower::ServiceExt;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("atlas-http-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    p
}

fn test_app(launch: &std::path::Path) -> axum::Router {
    let reg = ProjectRegistry::load_with_registry_path(launch, launch.join("test.registry.json"));
    let cfg = AtlasConfig::default();
    let state = build_state_with_registry(launch.to_path_buf(), cfg, "test-token".into(), reg);
    build_api_router(state)
}

fn auth_req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", "Bearer test-token")
        .body(Body::empty())
        .unwrap()
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(res.into_body(), 2 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn projects_list_requires_auth_and_includes_launch() {
    let launch = temp_dir("auth");
    let app = test_app(&launch);

    let unauth = Request::builder()
        .method("GET")
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(unauth).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let res = app
        .clone()
        .oneshot(auth_req("GET", "/api/projects"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let projects = v["projects"].as_array().cloned().unwrap_or_default();
    assert!(!projects.is_empty(), "launch project should be listed: {v}");
    assert!(projects.iter().any(|p| p["isLaunch"] == true));

    fs::remove_dir_all(&launch).ok();
}

#[tokio::test]
async fn runs_and_tree_accept_project_query() {
    let launch = temp_dir("read");
    // Minimal wiki so tree is non-empty.
    fs::create_dir_all(launch.join("atlas")).unwrap();
    fs::write(
        launch.join("atlas/quickstart.md"),
        "---\ntitle: Quickstart\ntype: Quickstart\n---\n# hi\n",
    )
    .unwrap();

    let app = test_app(&launch);

    let res = app
        .clone()
        .oneshot(auth_req("GET", "/api/runs?project=default"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert!(v["projectId"].is_string(), "runs should carry project: {v}");

    let res = app
        .clone()
        .oneshot(auth_req("GET", "/api/tree?project=default"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert!(v["project"].is_string(), "tree should carry project: {v}");
    assert!(v["tree"].is_object());

    fs::remove_dir_all(&launch).ok();
}

#[tokio::test]
async fn unknown_project_id_is_error() {
    let launch = temp_dir("unknown");
    let app = test_app(&launch);

    let res = app
        .clone()
        .oneshot(auth_req("GET", "/api/runs?project=no-such-project"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let v = body_json(res).await;
    let err = v["error"].as_str().unwrap_or_default();
    assert!(err.contains("unknown project"), "unexpected error: {err}");

    fs::remove_dir_all(&launch).ok();
}

#[tokio::test]
async fn update_unknown_project_errors() {
    let launch = temp_dir("upd");
    let app = test_app(&launch);

    let req = Request::builder()
        .method("POST")
        .uri("/api/projects/nope/update")
        .header("authorization", "Bearer test-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"mode":"update"}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let v = body_json(res).await;
    let err = v["error"].as_str().unwrap_or_default();
    assert!(err.contains("unknown project"), "unexpected: {err}");

    fs::remove_dir_all(&launch).ok();
}
