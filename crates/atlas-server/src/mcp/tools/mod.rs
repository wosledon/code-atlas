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
    /// Cached at process start; resolve/list hot-reload from disk via `repo_root`.
    #[allow(dead_code)]
    pub(crate) projects: ProjectRegistry,
}

pub(crate) fn resolve_ctx_project(ctx: &McpCtx, project: Option<&str>) -> Result<ProjectRef> {
    // Hot-reload registry from disk so `atlas project add` applies without
    // restarting the MCP process.
    let reg = ProjectRegistry::load_with_discovery(&ctx.repo_root);
    reg.resolve(project)
}

pub(crate) fn list_registry(ctx: &McpCtx) -> ProjectRegistry {
    ProjectRegistry::load_with_discovery(&ctx.repo_root)
}
