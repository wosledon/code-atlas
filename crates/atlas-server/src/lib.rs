use anyhow::Result;
use atlas_core::AtlasConfig;
use atlas_core::projects::ProjectRegistry;
use atlas_store::{SearchHit, Store};
use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tower_http::cors::CorsLayer;

mod assets;
mod chat;
mod common;
mod config;
mod graph;
pub mod mcp;
mod projects;
mod runs;
mod search;
mod tree;
mod web_ui;

use crate::assets::health;
use crate::chat::kb_chat;
use crate::config::{get_config, post_config};
use crate::graph::{graph_nodes, list_entities, neighborhood};
use crate::projects::{
    list_projects, register_project, trigger_project_update, unregister_project,
};
use crate::runs::{list_runs, trigger_update_with_project};
use crate::search::kb_search;
use crate::tree::{doc_tree, list_pages, read_page};
use crate::web_ui::{UiSource, fallback_page, pick_ui_source, serve_embedded};

pub use crate::web_ui::has_embedded_ui;

#[derive(Clone)]
pub struct AppState {
    /// Launch repository (process default). Prefer resolving via `projects`.
    pub repo_root: PathBuf,
    pub cfg: Arc<AtlasConfig>,
    pub atlas_root: PathBuf,
    /// Project registry cache (mtime hot-reload).
    pub projects: Arc<RwLock<ProjectRegistry>>,
}

/// Build app state. Registry path follows the atlas executable by default.
pub fn build_state(repo_root: PathBuf, cfg: AtlasConfig) -> AppState {
    let atlas_root = cfg.atlas_root(&repo_root);
    let registry = ProjectRegistry::load_with_discovery(&repo_root);
    AppState {
        repo_root,
        cfg: Arc::new(cfg),
        atlas_root,
        projects: Arc::new(RwLock::new(registry)),
    }
}

/// Build state with an injected registry (tests / custom registry path).
pub fn build_state_with_registry(
    repo_root: PathBuf,
    cfg: AtlasConfig,
    registry: ProjectRegistry,
) -> AppState {
    let atlas_root = cfg.atlas_root(&repo_root);
    AppState {
        repo_root,
        cfg: Arc::new(cfg),
        atlas_root,
        projects: Arc::new(RwLock::new(registry)),
    }
}

/// API router without UI fallback (used by `serve` and integration tests).
pub fn build_api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runs", get(list_runs))
        .route("/api/projects", get(list_projects).post(register_project))
        .route(
            "/api/projects/{id}",
            axum::routing::delete(unregister_project),
        )
        .route("/api/projects/{id}/update", post(trigger_project_update))
        .route("/api/entities", get(list_entities))
        .route("/api/graph/nodes", get(graph_nodes))
        .route("/api/graph/nodes/{id}/neighborhood", get(neighborhood))
        .route("/api/kb/search", get(kb_search))
        .route("/api/kb/chat", post(kb_chat))
        .route("/api/pages/{*path}", get(read_page))
        .route("/api/pages", get(list_pages))
        .route("/api/tree", get(doc_tree))
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/run/update", post(trigger_update_with_project))
        .layer(cors_layer())
        .with_state(state)
}

pub async fn serve(
    repo_root: PathBuf,
    cfg: AtlasConfig,
    port: u16,
    web_dist: Option<PathBuf>,
) -> Result<()> {
    let state = build_state(repo_root, cfg);

    let mut app = build_api_router(state);

    match pick_ui_source(web_dist.as_deref()) {
        UiSource::Disk(dist) => {
            tracing::info!("UI: disk {}", dist.display());
            // SPA: unknown paths fall back to index.html
            app = app.fallback_service(tower_http::services::ServeDir::new(&dist).fallback(
                tower_http::services::ServeFile::new(dist.join("index.html")),
            ));
        }
        UiSource::Embedded => {
            tracing::info!("UI: embedded in binary (gzip)");
            app = app.fallback(serve_embedded);
        }
        UiSource::Fallback => {
            tracing::warn!(
                "UI: built-in fallback page. Build `cd web && npm run build` then rebuild the CLI, or pass --web-dist."
            );
            app = app.fallback(get(fallback_page));
        }
    }

    // Bound on every interface so the wiki can be opened from a phone or another
    // machine on the same network; the printed URLs stay on loopback because that
    // is what works from the machine that started it.
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("atlas web listening on {addr} (all interfaces)");
    println!("Code Atlas UI: http://127.0.0.1:{port}/");
    println!("               http://localhost:{port}/");
    axum::serve(listener, app).await?;
    Ok(())
}

/// The bundler serves the SPA from the same origin, so CORS is only needed for
/// the Vite dev server. Allow localhost origins instead of `*` — a remote page
/// must not be able to talk to a local wiki.
fn cors_layer() -> CorsLayer {
    use axum::http::{HeaderValue, Method, header};
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &HeaderValue, _| {
                let o = origin.as_bytes();
                o.starts_with(b"http://localhost:") || o.starts_with(b"http://127.0.0.1:")
            },
        ))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}
