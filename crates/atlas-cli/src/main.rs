use anyhow::Result;
use atlas_core::projects::ProjectRegistry;
use atlas_core::{pipeline, AtlasConfig};
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use tracing_subscriber::fmt::MakeWriter;

/// Logs are written to stderr **with the progress bars parked first**, so a log
/// line never lands in the middle of a bar redraw. stdout stays reserved for a
/// command's actual output.
#[derive(Clone, Copy)]
struct ProgressAwareStderr;

impl<'a> MakeWriter<'a> for ProgressAwareStderr {
    type Writer = ProgressAwareStderr;
    fn make_writer(&'a self) -> Self::Writer {
        *self
    }
}

impl Write for ProgressAwareStderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        pipeline::with_suspended_bars(|| {
            let mut err = std::io::stderr().lock();
            err.write_all(buf)?;
            err.flush()?;
            Ok(buf.len())
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        pipeline::with_suspended_bars(|| std::io::stderr().flush())
    }
}

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

#[derive(Subcommand, Debug)]
enum ProjectCmd {
    /// List launch project + registered / discovered projects
    List,
    /// Register a repository root in atlas.projects.json
    Add {
        root: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a registered project (launch project cannot be removed)
    Remove { id: String },
    /// Scan external_root markers and merge new projects into the registry
    Discover,
}

struct ProjectTarget {
    id: String,
    repo_root: PathBuf,
    cfg: AtlasConfig,
}

fn resolve_target(launch_root: &Path, project: Option<&str>) -> Result<ProjectTarget> {
    let raw = project.map(str::trim).unwrap_or("");
    let launch_cfg = AtlasConfig::load(launch_root)?;
    if raw.is_empty() || raw == "default" {
        return Ok(ProjectTarget {
            id: atlas_core::repo_slug(launch_root),
            repo_root: launch_root.to_path_buf(),
            cfg: launch_cfg,
        });
    }
    let reg = ProjectRegistry::load_with_discovery(launch_root);
    let pref = reg.resolve(Some(raw))?;
    Ok(ProjectTarget {
        id: pref.id,
        repo_root: pref.root,
        cfg: pref.cfg,
    })
}

fn apply_overrides(cfg: &mut AtlasConfig, provider: Option<String>, model: Option<String>) {
    if let Some(p) = provider {
        cfg.llm.provider = p;
    }
    if let Some(m) = model {
        cfg.llm.model = m;
    }
}

fn print_result(res: &pipeline::RunResult, project: &str, ci: bool) {
    let mut v = serde_json::to_value(res).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("project".into(), project.into());
    }
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    if !ci {
        return;
    }
    // CI: fail the step on a failed run or on structural problems, while
    // tolerating advisory notes (e.g. template fallback without an LLM key).
    let hard_problem = res.notes.iter().any(|n| n.starts_with("problem:"));
    if res.status == "failed" || hard_problem {
        std::process::exit(1);
    }
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
        Cmd::Project { cmd } => match cmd {
            ProjectCmd::List => {
                let mut reg = ProjectRegistry::load_with_discovery(&launch_root);
                let discovered = reg.discover_projects();
                if !discovered.is_empty() {
                    eprintln!("discovered {} project(s) from external_root", discovered.len());
                }
                let list = reg.list();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "launch": reg.launch_id(),
                        "registry_path": reg.registry_path().display().to_string(),
                        "projects": list.iter().map(|p| serde_json::json!({
                            "id": p.id,
                            "name": p.name,
                            "root": p.root.display().to_string(),
                            "atlas_root": p.atlas_root.display().to_string(),
                            "is_launch": p.is_launch,
                        })).collect::<Vec<_>>(),
                    }))?
                );
            }
            ProjectCmd::Add { root, id, name } => {
                let mut reg = ProjectRegistry::load_with_discovery(&launch_root);
                let pref = reg.register(&root, id.as_deref(), name.as_deref())?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "id": pref.id,
                        "name": pref.name,
                        "root": pref.root.display().to_string(),
                        "registry_path": reg.registry_path().display().to_string(),
                    }))?
                );
            }
            ProjectCmd::Remove { id } => {
                let mut reg = ProjectRegistry::load_with_discovery(&launch_root);
                reg.unregister(&id)?;
                println!("removed project `{id}`");
            }
            ProjectCmd::Discover => {
                let mut reg = ProjectRegistry::load_with_discovery(&launch_root);
                let found = reg.discover_projects();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "discovered": found.iter().map(|e| serde_json::json!({
                            "id": e.id,
                            "root": e.root,
                        })).collect::<Vec<_>>(),
                        "projects": reg.list().iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
                    }))?
                );
            }
        },
        Cmd::Mcp => {
            let mut cfg = AtlasConfig::load(&launch_root)?;
            // MCP project resolution uses the registry; launch cfg is the fallback.
            apply_overrides(&mut cfg, None, None);
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

/// Locate the SPA build so `atlas web` works without extra setup.
/// Prefer an explicit `--web-dist`, then the target repo, then paths next to the binary
/// (covers `cargo build -p atlas-cli` from a source checkout and a simple install layout).
fn resolve_web_dist(explicit: Option<&std::path::Path>, repo_root: &std::path::Path) -> Option<PathBuf> {
    if let Some(d) = explicit {
        return d.join("index.html").exists().then(|| d.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = vec![repo_root.join("web/dist")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.extend([
                dir.join("web/dist"),
                dir.join("../web/dist"),
                dir.join("../../web/dist"),
                dir.join("../../../web/dist"),
            ]);
        }
    }
    candidates
        .into_iter()
        .find(|d| d.join("index.html").exists())
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    if !src.exists() {
        anyhow::bail!("wiki root missing: {}", src.display());
    }
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
