use anyhow::Result;
use atlas_core::{pipeline, AtlasConfig};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    /// Serve local UI + API
    Serve {
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
        Cmd::Serve { port, insecure, web_dist } => {
            let dist = web_dist.or_else(|| {
                let d = repo_root.join("web/dist");
                d.exists().then_some(d)
            });
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
