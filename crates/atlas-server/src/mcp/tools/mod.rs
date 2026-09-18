//! MCP tool catalogue and handlers for document maintenance.
//!
//! Split by responsibility: [`catalog`] schemas, [`handlers`] dispatch,
//! [`pages`] wiki page IO, [`projects`] registry helpers, [`args`] parsers.

mod args;
mod catalog;
mod handlers;
mod pages;
mod projects;

use anyhow::Result;
use atlas_core::projects::{ProjectRef, ProjectRegistry};
use atlas_core::AtlasConfig;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) use args::{bool_arg, int_arg, str_arg};
pub(crate) use catalog::tool_defs;
pub(crate) use handlers::call_tool;
pub(crate) use pages::list_pages;
pub(crate) use projects::{list_projects_json, repo_info};

pub(crate) struct McpCtx {
    /// Launch repo (fallback identity; prefer resolving via `projects`).
    #[allow(dead_code)]
    pub(crate) repo_root: PathBuf,
    #[allow(dead_code)]
    pub(crate) cfg: AtlasConfig,
    #[allow(dead_code)]
    pub(crate) atlas_root: PathBuf,
    /// Cached registry; hot-reloaded via mtime on each resolve.
    pub(crate) projects: Mutex<ProjectRegistry>,
}

pub(crate) fn resolve_ctx_project(ctx: &McpCtx, project: Option<&str>) -> Result<ProjectRef> {
    let mut reg = ctx.projects.lock().unwrap_or_else(|e| e.into_inner());
    reg.refresh_if_changed();
    reg.discover_throttled();
    reg.resolve(project)
}

pub(crate) fn list_registry(ctx: &McpCtx) -> ProjectRegistry {
    let mut reg = ctx.projects.lock().unwrap_or_else(|e| e.into_inner()).clone();
    reg.refresh_if_changed();
    reg.discover_projects();
    if let Ok(mut guard) = ctx.projects.lock() {
        *guard = reg.clone();
    }
    reg
}
