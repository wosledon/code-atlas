//! Run orchestration for `atlas init|update`.
//!
//! [`run_init_or_update`] owns the sequence of phases and nothing else: the
//! session (lock, store, git state) lives here, while seeding the graph,
//! preparing jobs, generating pages, writing them and finishing the run are
//! separate modules so each stage can be read on its own.

use super::*;

use super::generate::generate_pages;
use super::graph::seed_graph;
use super::maintenance::{finalize, read_last_update};
use super::plan::plan_pages;
use super::prepare::prepare_jobs;
use std::sync::Arc;

/// State that must outlive every phase of a run: the exclusive lock (released
/// when the run row is finished), the store handle and the repository facts
/// captured at start-up.
pub(super) struct RunSession {
    pub(super) run_id: String,
    pub(super) mode: String,
    pub(super) atlas_root: PathBuf,
    pub(super) git_head: Option<String>,
    pub(super) last: Option<LastUpdate>,
    pub(super) store: Store,
    _lock: RunLock,
}

/// Everything the phases need besides the session: the config, the repository
/// scan, the page plan and the LLM client chosen for this run.
pub(super) struct RunContext<'a> {
    pub(super) ctx: &'a PipelineCtx,
    pub(super) session: &'a RunSession,
    pub(super) scan: &'a Arc<RepoScan>,
    pub(super) plan: &'a [PlannedPage],
    pub(super) llm: &'a Arc<LlmClient>,
    pub(super) use_llm: bool,
}

/// Accumulators filled in while the run progresses and consumed when the final
/// [`RunResult`] is assembled.
#[derive(Default)]
pub(super) struct RunCounts {
    pub(super) notes: Vec<String>,
    pub(super) pages_written: Vec<String>,
    pub(super) prompt_tokens: i64,
    pub(super) completion_tokens: i64,
    pub(super) pages_reused: usize,
    pub(super) reused_chunks: usize,
    /// Pages that fell back to the structural template (no usable model output).
    pub(super) template_pages: usize,
    /// Pages whose previous body was kept because regenerating them failed.
    pub(super) pages_kept: usize,
}

pub async fn run_init_or_update(
    ctx: &PipelineCtx,
    mode: &str,
    instruction: Option<&str>,
) -> Result<RunResult> {
    let session = RunSession::start(ctx, mode)?;
    let result = execute(&session, ctx, instruction).await;
    session.finish(result)
}

/// Walk the phases of a run. An `update` whose git head is unchanged and whose
/// worktree is clean short-circuits into a no-op result.
async fn execute(
    session: &RunSession,
    ctx: &PipelineCtx,
    instruction: Option<&str>,
) -> Result<RunResult> {
    if let Some(no_op) = session.no_op(ctx)? {
        return Ok(no_op);
    }

    let mut counts = RunCounts::default();
    let scan = Arc::new(scan_repo_with_skips(
        &ctx.repo_root,
        &ctx.cfg.ignored_scan_files(&ctx.repo_root),
    )?);
    report_scan(&scan);
    let llm = Arc::new(build_llm(ctx)?);
    let use_llm = llm.configured() && !llm.is_host_agent();
    if !use_llm {
        // Never let a keyless run silently rewrite the wiki with templates: say
        // why the model is unavailable and what to set.
        let reason = llm.unavailable_reason();
        counts.notes.push(format!("template mode: {reason}"));
        println!("[atlas] ⚠ 未启用 LLM，本次只生成结构模板：{reason}");
        println!("[atlas]   已存在的模型正文会原样保留，设置 key 后重跑 `atlas update` 即可重写。");
    }
    let plan = plan_pages(&scan, &session.mode, instruction);
    let run = RunContext {
        ctx,
        session,
        scan: &scan,
        plan: &plan,
        llm: &llm,
        use_llm,
    };

    seed_graph(&run)?;
    let jobs = prepare_jobs(&run)?;
    // Generation and writing are interleaved inside `generate_pages`: every page
    // is written the moment it is ready.
    generate_pages(&run, jobs, &mut counts).await?;
    finalize(&run, counts)
}

impl RunSession {
    /// Take the run lock, open the store, mark the run as `running` and capture
    /// the repository state the phases compare against.
    fn start(ctx: &PipelineCtx, mode: &str) -> Result<Self> {
        let run_id = Uuid::new_v4().to_string();
        let data_dir = match ctx.cfg.output.strategy.as_str() {
            "in-repo" => ctx.repo_root.join("data"),
            _ => ctx
                .cfg
                .atlas_root(&ctx.repo_root)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| ctx.repo_root.join(".atlas-data")),
        };

        let lock = RunLock::acquire(&data_dir, &run_id)?;
        let store = Store::open(&ctx.cfg.db_path(&ctx.repo_root))?;
        // The lock is exclusive, so any leftover `running` row belongs to a dead process.
        if let Ok(n) = store.fail_stale_runs(0) {
            if n > 0 {
                tracing::warn!("reconciled {n} stale run(s) still marked as running");
            }
        }
        let atlas_root = ctx.cfg.atlas_root(&ctx.repo_root);
        let last = read_last_update(&atlas_root);
        spawn_heartbeat(&data_dir, &run_id);
        let git_head = git::git_head(&ctx.repo_root);
        store.begin_run(
            &run_id,
            mode,
            Some(&ctx.cfg.llm.provider),
            Some(&ctx.cfg.llm.model),
            Some(&ctx.cfg.output.language),
        )?;

        Ok(Self {
            run_id,
            mode: mode.to_string(),
            atlas_root,
            git_head,
            last,
            store,
            _lock: lock,
        })
    }

    /// `Some(result)` when this `update` has nothing to do: the previous run was
    /// on the same git head and the worktree has no relevant changes.
    fn no_op(&self, ctx: &PipelineCtx) -> Result<Option<RunResult>> {
        if self.mode != "update" {
            return Ok(None);
        }
        let Some(prev) = &self.last else {
            // never ran before: an update behaves like an init
            return Ok(None);
        };
        let (Some(prev_h), Some(cur_h)) = (&prev.gitHead, &self.git_head) else {
            return Ok(None);
        };
        let status = git::git_status_short(&ctx.repo_root);
        let dirty = status
            .iter()
            .any(|l| !l.contains("atlas/") && !l.contains("data/") && !l.contains(".atlas-data"));
        if prev_h != cur_h || dirty {
            return Ok(None);
        }
        Ok(Some(RunResult {
            run_id: self.run_id.clone(),
            mode: self.mode.clone(),
            status: "no_op".into(),
            pages_written: vec![],
            entities: 0,
            chunks: 0,
            no_op: true,
            prompt_tokens: 0,
            completion_tokens: 0,
            notes: vec!["git head unchanged and worktree clean".into()],
        }))
    }

    /// Record the run outcome (and release the lock) before handing the result
    /// back to the caller.
    fn finish(self, result: Result<RunResult>) -> Result<RunResult> {
        match result {
            Ok(res) => {
                let status = if res.no_op { "no_op" } else { "succeeded" };
                self.store.finish_run(
                    &self.run_id,
                    status,
                    self.git_head.as_deref(),
                    res.prompt_tokens,
                    res.completion_tokens,
                    0.0,
                    None,
                )?;
                Ok(res)
            }
            Err(e) => {
                self.store.finish_run(
                    &self.run_id,
                    "failed",
                    self.git_head.as_deref(),
                    0,
                    0,
                    0.0,
                    Some(&format!("{e:#}")),
                )?;
                Err(e)
            }
        }
    }
}

/// Keep the lock file's heartbeat fresh during long LLM runs, so a watcher can
/// tell a slow run from a dead one.
fn spawn_heartbeat(data_dir: &Path, run_id: &str) {
    let data_dir = data_dir.to_path_buf();
    let run_id = run_id.to_string();
    std::thread::spawn(move || {
        let hb = data_dir.join("atlas.lock.heartbeat");
        for _ in 0..3600 {
            std::thread::sleep(std::time::Duration::from_secs(20));
            if !hb.exists() {
                break;
            }
            let _ = std::fs::write(&hb, run_id.as_bytes());
        }
    });
}

fn report_scan(scan: &RepoScan) {
    let files = scan.files.iter().filter(|f| f.language.is_some()).count();
    tracing::info!("扫描完成：{files} 源文件 · 语言 {:?}", scan.languages);
    println!("[atlas] 扫描 {files} 个源文件 · 语言 {:?}", scan.languages);
}

fn build_llm(ctx: &PipelineCtx) -> Result<LlmClient> {
    let llm_cfg = LlmConfig {
        provider: ctx.cfg.llm.provider.clone(),
        model: ctx.cfg.llm.model.clone(),
        base_url: ctx.cfg.llm.base_url.clone(),
        temperature: ctx.cfg.llm.temperature,
        max_output_tokens: ctx.cfg.llm.max_output_tokens.max(4096),
        timeout_secs: ctx.cfg.llm.timeout_secs.max(120),
        retries: ctx.cfg.llm.retries.max(1),
    };
    LlmClient::from_env(llm_cfg)
}
