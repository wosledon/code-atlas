//! Producing the body of a single page: one model call, the optional depth
//! rewrite, and the fallbacks (reuse, keep-previous, structural template).
//!
//! Concurrency, progress bars and streaming the results into the writer live in
//! [`super::generate`]; this module only answers "what body does this page get".

use super::brief::depth_gaps;
use super::prompt::{expand_messages, generate_messages};
use super::template::template_page_body;
use super::types::{GeneratedPageWithMeta, PageJob, PageOutcome};
use super::*;

use indicatif::ProgressBar;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// State shared by every page task plus the counters the run summary reads once
/// the generation stage is over.
#[derive(Clone)]
pub(super) struct PageGen {
    llm: Arc<LlmClient>,
    scan: Arc<RepoScan>,
    cfg: Arc<AtlasConfig>,
    tools: Arc<RepoTools>,
    use_llm: bool,
    total_pages: u64,
    counters: Arc<Counters>,
}

#[derive(Default)]
struct Counters {
    ok: AtomicUsize,
    fail: AtomicUsize,
    kept: AtomicUsize,
    expanded: AtomicUsize,
    first_error: Mutex<Option<String>>,
}

impl PageGen {
    pub(super) fn new(
        llm: Arc<LlmClient>,
        scan: Arc<RepoScan>,
        cfg: Arc<AtlasConfig>,
        tools: Arc<RepoTools>,
        use_llm: bool,
        total_pages: u64,
    ) -> Self {
        Self {
            llm,
            scan,
            cfg,
            tools,
            use_llm,
            total_pages,
            counters: Arc::new(Counters::default()),
        }
    }

    /// Pages the model produced.
    pub(super) fn ok(&self) -> usize {
        self.counters.ok.load(Ordering::Relaxed)
    }

    /// Pages that did not get a fresh model body (kept previous or templated).
    pub(super) fn failed(&self) -> usize {
        self.counters.fail.load(Ordering::Relaxed)
    }

    /// Pages whose previous body was kept because regenerating them failed.
    pub(super) fn kept(&self) -> usize {
        self.counters.kept.load(Ordering::Relaxed)
    }

    /// Pages rewritten by the depth gate.
    pub(super) fn expanded(&self) -> usize {
        self.counters.expanded.load(Ordering::Relaxed)
    }

    pub(super) fn first_error(&self) -> Option<String> {
        self.counters
            .first_error
            .lock()
            .ok()
            .and_then(|s| s.clone())
    }

    /// Produce the body of one page and report progress on `sp` (`n` is its
    /// 1-based position in the plan).
    pub(super) async fn run(
        &self,
        n: usize,
        job: PageJob,
        sp: &ProgressBar,
    ) -> GeneratedPageWithMeta {
        let PageJob {
            page,
            evidence,
            fingerprint,
            reuse_body,
            keep_body_on_failure,
        } = job;
        let total = self.total_pages;
        let started = std::time::Instant::now();

        if let Some(body) = reuse_body {
            sp.finish_with_message(format!(
                "↻ {n}/{total} {} · 复用（证据未变）",
                page.rel_path
            ));
            self.counters.ok.fetch_add(1, Ordering::Relaxed);
            return GeneratedPageWithMeta {
                page,
                body,
                usage: None,
                fingerprint,
                outcome: PageOutcome::Reused,
            };
        }

        sp.set_message(format!("撰页 {n}/{total} {}", page.rel_path));
        tracing::info!("[{n}/{total}] 生成 {}", page.rel_path);

        if !self.use_llm {
            // Keep whatever the last real run produced: a key-less run must not
            // replace model-written pages with structural templates.
            if keep_body_on_failure {
                return self.keep_previous(&page, &fingerprint, None, "模板模式", sp, n, started);
            }
            sp.finish_with_message(format!("✓ {n}/{total} {} · 模板", page.rel_path));
            return self.templated(page, fingerprint, None);
        }

        match generate_page_with_llm(&self.llm, &evidence, &page, &self.cfg, &self.tools).await {
            Ok((usage, body)) if body.trim().chars().count() < 80 => {
                self.counters.fail.fetch_add(1, Ordering::Relaxed);
                if keep_body_on_failure {
                    return self.keep_previous(&page, &fingerprint, Some(usage), "输出过短", sp, n, started);
                }
                sp.finish_with_message(format!(
                    "~ {n}/{total} {} · 模板回退 · {:.1}s",
                    page.rel_path,
                    started.elapsed().as_secs_f64()
                ));
                self.templated(page, fingerprint, Some(usage))
            }
            Ok((usage, body)) => {
                let (body, usage, expanded) = if self.cfg.llm.depth_pass {
                    deepen_page(
                        &self.llm,
                        &self.tools,
                        &self.cfg,
                        &page,
                        &evidence,
                        body,
                        usage,
                        sp,
                        n,
                        total,
                    )
                    .await
                } else {
                    (body, usage, false)
                };
                if expanded {
                    self.counters.expanded.fetch_add(1, Ordering::Relaxed);
                }
                self.counters.ok.fetch_add(1, Ordering::Relaxed);
                sp.finish_with_message(format!(
                    "✓ {n}/{total} {} · tokens {}+{} · {:.1}s{}",
                    page.rel_path,
                    usage.0,
                    usage.1,
                    usage.2 as f64 / 1000.0,
                    if expanded { " · 深度重写" } else { "" }
                ));
                GeneratedPageWithMeta {
                    page,
                    body,
                    usage: Some(usage),
                    fingerprint,
                    outcome: PageOutcome::Generated,
                }
            }
            Err(e) => {
                self.counters.fail.fetch_add(1, Ordering::Relaxed);
                let msg = format!("{e:#}");
                let short: String = msg.chars().take(100).collect();
                if let Ok(mut slot) = self.counters.first_error.lock()
                    && slot.is_none()
                {
                    *slot = Some(msg.chars().take(400).collect());
                }
                tracing::warn!("[{n}/{total}] {} 回退: {e:#}", page.rel_path);
                if keep_body_on_failure {
                    return self.keep_previous(
                        &page,
                        &fingerprint,
                        None,
                        &format!("生成失败 · {short}"),
                        sp,
                        n,
                        started,
                    );
                }
                sp.finish_with_message(format!(
                    "! {n}/{total} {} · 回退模板 · {short}",
                    page.rel_path
                ));
                self.templated(page, fingerprint, None)
            }
        }
    }

    /// Structural fallback: only used when the page has nothing better to show.
    fn templated(
        &self,
        page: PlannedPage,
        fingerprint: String,
        usage: Option<(i64, i64, i64)>,
    ) -> GeneratedPageWithMeta {
        let body = template_page_body(&self.scan, &page, &self.cfg);
        GeneratedPageWithMeta {
            page,
            body,
            usage,
            fingerprint: template_fingerprint(&fingerprint),
            outcome: PageOutcome::Template,
        }
    }

    /// Leave the previous version of a page alone: the writer skips it, so its
    /// file, claims and chunks stay untouched and the next run retries it (the
    /// stored fingerprint still differs from the current evidence).
    #[allow(clippy::too_many_arguments)]
    fn keep_previous(
        &self,
        page: &PlannedPage,
        fingerprint: &str,
        usage: Option<(i64, i64, i64)>,
        label: &str,
        sp: &ProgressBar,
        n: usize,
        started: std::time::Instant,
    ) -> GeneratedPageWithMeta {
        self.counters.kept.fetch_add(1, Ordering::Relaxed);
        sp.finish_with_message(format!(
            "≣ {n}/{} {} · {label} · 保留上一版 · {:.1}s",
            self.total_pages,
            page.rel_path,
            started.elapsed().as_secs_f64()
        ));
        GeneratedPageWithMeta {
            page: page.clone(),
            body: String::new(),
            usage,
            fingerprint: fingerprint.to_string(),
            outcome: PageOutcome::KeptPrevious,
        }
    }
}

async fn generate_page_with_llm(
    llm: &LlmClient,
    evidence: &str,
    page: &PlannedPage,
    cfg: &AtlasConfig,
    tools: &RepoTools,
) -> Result<((i64, i64, i64), String)> {
    let (system, user) = generate_messages(cfg, page, evidence);
    call_model(llm, tools, cfg, &system, &user).await
}

/// 「扩写」：深度门发现页面太浅时，让模型带着缺口清单重写全文。
#[allow(clippy::too_many_arguments)]
async fn expand_page_with_llm(
    llm: &LlmClient,
    evidence: &str,
    page: &PlannedPage,
    cfg: &AtlasConfig,
    tools: &RepoTools,
    draft: &str,
    gaps: &[String],
) -> Result<((i64, i64, i64), String)> {
    let (system, user) = expand_messages(cfg, page, gaps, draft, evidence);
    call_model(llm, tools, cfg, &system, &user).await
}

/// One model call with the repository tools attached (or a plain chat when tools
/// are disabled). Returns the token/latency usage together with the text.
async fn call_model(
    llm: &LlmClient,
    tools: &RepoTools,
    cfg: &AtlasConfig,
    system: &str,
    user: &str,
) -> Result<((i64, i64, i64), String)> {
    let rounds = cfg.llm.max_tool_rounds;
    if rounds > 0 && llm.supports_tools() {
        let specs = RepoTools::specs();
        match llm
            .chat_with_tools(system, user, &specs, rounds, |name, args| tools.call(name, args))
            .await
        {
            Ok(resp) => return Ok(usage_of(&resp)),
            // A gateway may reject the tool protocol itself (empty follow-up
            // round, schema validation, ...). Retry once tool-free: the model
            // still has the evidence bundle, and a page written from evidence
            // beats a structural template.
            Err(e) => tracing::warn!("工具回合失败，改为无工具重试：{e:#}"),
        }
    }
    Ok(usage_of(&llm.chat(system, user).await?))
}

fn usage_of(resp: &atlas_llm::LlmResponse) -> ((i64, i64, i64), String) {
    (
        (
            resp.usage.prompt_tokens,
            resp.usage.completion_tokens,
            resp.usage.latency_ms,
        ),
        resp.text.clone(),
    )
}

/// Pages whose body came from the structural template are *not* up to date:
/// poisoning the stored fingerprint makes the next `atlas update` ask the model
/// again instead of silently reusing template text (which is what happened after
/// a failed LLM run or a key-less template run).
fn template_fingerprint(fingerprint: &str) -> String {
    format!("{fingerprint}+template")
}

/// 深度门 + 一次扩写：正文通过则原样返回，未通过则尝试重写，取缺口更少的版本。
/// 返回（最终正文, 累计用量, 是否真的重写了）。
#[allow(clippy::too_many_arguments)]
async fn deepen_page(
    llm: &LlmClient,
    tools: &RepoTools,
    cfg: &AtlasConfig,
    page: &PlannedPage,
    evidence: &str,
    body: String,
    usage: (i64, i64, i64),
    sp: &ProgressBar,
    n: usize,
    total_pages: u64,
) -> (String, (i64, i64, i64), bool) {
    let gaps = depth_gaps(&body, page);
    if gaps.is_empty() {
        return (body, usage, false);
    }
    sp.set_message(format!(
        "扩写 {n}/{total_pages} {} · {} 项待补",
        page.rel_path,
        gaps.len()
    ));
    tracing::info!(
        "[{n}/{total_pages}] 深度门未过（{} 项），扩写 {}",
        gaps.len(),
        page.rel_path
    );
    match expand_page_with_llm(llm, evidence, page, cfg, tools, &body, &gaps).await {
        Ok((extra, revised)) => {
            let total = (
                usage.0 + extra.0,
                usage.1 + extra.1,
                usage.2 + extra.2,
            );
            let after = depth_gaps(&revised, page);
            let improved = revised.trim().chars().count() >= 80
                && (after.len() < gaps.len()
                    || (after.len() == gaps.len() && revised.chars().count() > body.chars().count()));
            if improved {
                (revised, total, true)
            } else {
                (body, total, false)
            }
        }
        Err(e) => {
            tracing::warn!("[{n}/{total_pages}] {} 扩写失败: {e:#}", page.rel_path);
            (body, usage, false)
        }
    }
}
