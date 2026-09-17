use anyhow::Result;
use atlas_core::AtlasConfig;
use atlas_store::{SearchHit, Store};
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

mod assets;
mod auth;
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
use crate::projects::list_projects;
use crate::runs::{list_runs, trigger_update};
use crate::search::kb_search;
use crate::tree::{doc_tree, list_pages, read_page};
use crate::web_ui::{fallback_page, pick_ui_source, serve_embedded, UiSource};

pub use crate::web_ui::has_embedded_ui;

#[derive(Clone)]
pub struct AppState {
    pub repo_root: PathBuf,
    pub cfg: Arc<AtlasConfig>,
    pub token: Arc<String>,
    pub atlas_root: PathBuf,
}

pub async fn serve(
    repo_root: PathBuf,
    cfg: AtlasConfig,
    port: u16,
    insecure: bool,
    web_dist: Option<PathBuf>,
) -> Result<()> {
    let atlas_root = cfg.atlas_root(&repo_root);
    let token = if insecure {
        "insecure".to_string()
    } else {
        uuid::Uuid::new_v4().simple().to_string()
    };
    let state = AppState {
        repo_root,
        cfg: Arc::new(cfg),
        token: Arc::new(token.clone()),
        atlas_root,
    };

    let mut app = Router::new()
        .route("/api/health", get(health))
        .route("/api/runs", get(list_runs))
        .route("/api/projects", get(list_projects))
        .route("/api/entities", get(list_entities))
        .route("/api/graph/nodes", get(graph_nodes))
        .route("/api/graph/nodes/{id}/neighborhood", get(neighborhood))
        .route("/api/kb/search", get(kb_search))
        .route("/api/kb/chat", post(kb_chat))
        .route("/api/pages/{*path}", get(read_page))
        .route("/api/pages", get(list_pages))
        .route("/api/tree", get(doc_tree))
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/run/update", post(trigger_update))
        .layer(cors_layer())
        .with_state(state);

    match pick_ui_source(web_dist.as_deref()) {
        UiSource::Disk(dist) => {
            tracing::info!("UI: disk {}", dist.display());
            // SPA: unknown paths fall back to index.html
            app = app.fallback_service(
                tower_http::services::ServeDir::new(&dist)
                    .fallback(tower_http::services::ServeFile::new(dist.join("index.html"))),
            );
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

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("atlas web on http://{addr}/?t={token}");
    println!("Code Atlas UI: http://{addr}/?t={token}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// The bundler serves the SPA from the same origin, so CORS is only needed for
/// the Vite dev server. Allow localhost origins instead of `*` — a remote page
/// must not be able to talk to a local wiki.
fn cors_layer() -> CorsLayer {
    use axum::http::{header, HeaderValue, Method};
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &HeaderValue, _| {
                let o = origin.as_bytes();
                o.starts_with(b"http://localhost:") || o.starts_with(b"http://127.0.0.1:")
            },
        ))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}
