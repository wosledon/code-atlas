//! Code Atlas CLI — clap entry and command dispatch.

mod fsutil;
mod logging;
mod project_cmd;
mod target;

use anyhow::Result;
use atlas_core::{pipeline, AtlasConfig};
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::path::PathBuf;

use crate::fsutil::{copy_dir, resolve_web_dist};
use crate::logging::ProgressAwareStderr;
use crate::project_cmd::{run_project_cmd, ProjectCmd};
use crate::target::{apply_overrides, print_result, resolve_target};

#[derive(Parser, Debug)]
#[command(name = "atlas", version, about = "Code Atlas — living wiki, knowledge graph & KB")]
struct Cli {
    /// Repository / hub root (default: auto-detect)
    #[arg(long, global = true)]
    path: Option<PathBuf>,

    /// Project id from the registry (default: current / launch project)
    #[arg(long, global = true)]
    project: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Initialize wiki + KB
    Init {
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        instruction: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Incremental update (project-scoped)
    Update {
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        instruction: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Search knowledge base
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Show last runs
    Status {
        #[arg(long)]
        last: bool,
    },
    /// Rebuild SQLite index from markdown
    Reindex,
    /// Preview planned documentation tree (no LLM)
    Plan {
        #[arg(long)]
        instruction: Option<String>,
    },
    /// Start local web UI + API (CLI is the only entry)
    #[command(visible_alias = "serve")]
    Web {
        #[arg(long, default_value_t = 4321)]
        port: u16,
        #[arg(long)]
        insecure: bool,
        #[arg(long)]
        web_dist: Option<PathBuf>,
    },
    /// Export wiki markdown directory
    Export {
        out: PathBuf,
    },
    /// Read-only integrity check
    Check,
    /// Generate atlas.toml (api_key optional; env vars still override)
    #[command(visible_alias = "config")]
    InitConfig {
        /// Write atlas.toml.example instead of atlas.toml
        #[arg(long)]
        example: bool,
        /// Overwrite if the file already exists
        #[arg(long)]
        force: bool,
    },
    /// Manage the project registry (project-dimension update targets)
    #[command(visible_alias = "projects")]
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// Start MCP server on stdio (search / read page / status / plan / update)
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(ProgressAwareStderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let launch_root = atlas_core::resolve_repo_root(cli.path.as_deref())?;

    match cli.cmd {
        Cmd::InitConfig { example, force } => {
            let name = if example { "atlas.toml.example" } else { "atlas.toml" };
            let p = atlas_core::write_config_file(&launch_root, name, force)?;
            println!("wrote {}", p.display());
        }
        Cmd::Project { cmd } => run_project_cmd(&launch_root, cmd)?,
        Cmd::Mcp => {
            let cfg = AtlasConfig::load(&launch_root)?;
            atlas_server::mcp::serve_stdio(launch_root, cfg).await?;
        }
        Cmd::Init { ci, instruction, provider, model } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let mut cfg = target.cfg;
            apply_overrides(&mut cfg, provider, model);
            let ctx = pipeline::PipelineCtx {
                repo_root: target.repo_root,
                cfg,
            };
            let res = pipeline::run_init_or_update(&ctx, "init", instruction.as_deref()).await?;
            print_result(&res, &target.id, ci);
        }
        Cmd::Update { ci, instruction, provider, model } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let mut cfg = target.cfg;
            apply_overrides(&mut cfg, provider, model);
            let ctx = pipeline::PipelineCtx {
                repo_root: target.repo_root,
                cfg,
            };
            let res = pipeline::run_init_or_update(&ctx, "update", instruction.as_deref()).await?;
            print_result(&res, &target.id, ci);
        }
        Cmd::Search { query, limit } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let store = pipeline::open_store(&target.repo_root, &target.cfg)?;
            let hits = store.search(
                &query,
                limit,
                atlas_store::SearchMode::parse(&target.cfg.kb.search),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "project": target.id,
                    "hits": hits,
                }))?
            );
        }
        Cmd::Status { last } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let store = pipeline::open_store(&target.repo_root, &target.cfg)?;
            let runs = store.list_runs(if last { 1 } else { 20 })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "project": target.id,
                    "runs": runs,
                }))?
            );
        }
        Cmd::Reindex => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let n = pipeline::reindex(&target.repo_root, &target.cfg)?;
            println!("reindexed {n} pages for project `{}`", target.id);
        }
        Cmd::Plan { instruction } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let pages = pipeline::preview_plan(&target.repo_root, instruction.as_deref())?;
            println!(
                "planned {} pages for project `{}` ({})",
                pages.len(),
                target.id,
                target.repo_root.display()
            );
            for p in pages {
                println!("  {}\t{}\t{}", p.rel_path, p.page_type, p.description);
            }
        }
        Cmd::Web { port, insecure, web_dist } => {
            let cfg = AtlasConfig::load(&launch_root)?;
            let dist = resolve_web_dist(web_dist.as_deref(), &launch_root);
            if dist.is_none() && !atlas_server::has_embedded_ui() {
                tracing::warn!(
                    "web/dist not found and not embedded; using built-in fallback UI. \
                     Build with `cd web && npm run build` then rebuild, or pass --web-dist."
                );
            }
            atlas_server::serve(launch_root, cfg, port, insecure, dist).await?;
        }
        Cmd::Export { out } => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let atlas_root = target.cfg.atlas_root(&target.repo_root);
            std::fs::create_dir_all(&out)?;
            copy_dir(&atlas_root, &out)?;
            println!(
                "exported {} ({}) -> {}",
                target.id,
                atlas_root.display(),
                out.display()
            );
        }
        Cmd::Check => {
            let target = resolve_target(&launch_root, cli.project.as_deref())?;
            let issues = pipeline::run_check(&target.repo_root, &target.cfg)?;
            if issues.is_empty() {
                println!("ok ({})", target.id);
            } else {
                for i in issues {
                    println!("broken: {i}");
                }
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
