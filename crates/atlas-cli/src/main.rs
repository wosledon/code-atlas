use anyhow::Result;
use atlas_core::{pipeline, AtlasConfig};
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
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
    /// Repository root (default: auto-detect)
    #[arg(long, global = true)]
    path: Option<PathBuf>,

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
    /// Incremental update
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
    /// Write atlas.toml.example
    InitConfig,
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
    let repo_root = atlas_core::resolve_repo_root(cli.path.as_deref())?;
    let mut cfg = AtlasConfig::load(&repo_root)?;

    match cli.cmd {
        Cmd::InitConfig => {
            let p = atlas_core::write_example_config(&repo_root)?;
            println!("wrote {}", p.display());
        }
        Cmd::Init { ci, instruction, provider, model } => {
            apply_overrides(&mut cfg, provider, model);
            let ctx = pipeline::PipelineCtx {
                repo_root: repo_root.clone(),
                cfg,
            };
            let res = pipeline::run_init_or_update(&ctx, "init", instruction.as_deref()).await?;
            print_result(&res, ci);
        }
        Cmd::Update { ci, instruction, provider, model } => {
            apply_overrides(&mut cfg, provider, model);
            let ctx = pipeline::PipelineCtx {
                repo_root: repo_root.clone(),
                cfg,
            };
            let res = pipeline::run_init_or_update(&ctx, "update", instruction.as_deref()).await?;
            print_result(&res, ci);
        }
        Cmd::Search { query, limit } => {
            let store = pipeline::open_store(&repo_root, &cfg)?;
            let hits = store.search(
                &query,
                limit,
                atlas_store::SearchMode::parse(&cfg.kb.search),
            )?;
            println!("{}", serde_json::to_string_pretty(&hits)?);
        }
        Cmd::Status { last } => {
            let store = pipeline::open_store(&repo_root, &cfg)?;
            let runs = store.list_runs(if last { 1 } else { 20 })?;
            println!("{}", serde_json::to_string_pretty(&runs)?);
        }
        Cmd::Reindex => {
            let n = pipeline::reindex(&repo_root, &cfg)?;
            println!("reindexed {n} pages");
        }
        Cmd::Plan { instruction } => {
            let pages = pipeline::preview_plan(&repo_root, instruction.as_deref())?;
            println!("planned {} pages for {}", pages.len(), repo_root.display());
            for p in pages {
                println!("  {}\t{}\t{}", p.rel_path, p.page_type, p.description);
            }
        }
        Cmd::Web { port, insecure, web_dist } => {
            let dist = resolve_web_dist(web_dist.as_deref(), &repo_root);
            if dist.is_none() && !atlas_server::has_embedded_ui() {
                tracing::warn!(
                    "web/dist not found and not embedded; using built-in fallback UI. \
                     Build with `cd web && npm run build` then rebuild, or pass --web-dist."
                );
            }
            atlas_server::serve(repo_root, cfg, port, insecure, dist).await?;
        }
        Cmd::Export { out } => {
            let atlas_root = cfg.atlas_root(&repo_root);
            std::fs::create_dir_all(&out)?;
            copy_dir(&atlas_root, &out)?;
            println!("exported {} -> {}", atlas_root.display(), out.display());
        }
        Cmd::Check => {
            let issues = pipeline::run_check(&repo_root, &cfg)?;
            if issues.is_empty() {
                println!("ok");
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

fn apply_overrides(cfg: &mut AtlasConfig, provider: Option<String>, model: Option<String>) {
    if let Some(p) = provider {
        cfg.llm.provider = p;
    }
    if let Some(m) = model {
        cfg.llm.model = m;
    }
}

fn print_result(res: &pipeline::RunResult, ci: bool) {
    println!("{}", serde_json::to_string_pretty(res).unwrap_or_default());
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
