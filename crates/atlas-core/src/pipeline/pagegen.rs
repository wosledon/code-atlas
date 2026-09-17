//! Producing the body of a single page: one model call, the optional depth
//! rewrite, and the fallbacks (reuse, keep-previous, structural template).
//!
//! Concurrency, progress bars and streaming the results into the writer live in
//! [`super::generate`]; this module only answers "what body does this page get".

use super::deepen::deepen_page;
use super::model_call::call_model;
use super::progress::PageBar;
use super::prompt::generate_messages;
use super::template::template_page_body;
use super::types::{GeneratedPageWithMeta, PageJob, PageOutcome};
use super::write::PageWriter;
use super::*;

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

    /// Produce the body of one page and report progress on `pb` (`n` is its
    /// 1-based position in the plan).
    ///
    /// When `writer` is set, the **first draft is flushed to disk immediately**
    /// (and again after an optional depth rewrite), so a long page never blocks
    /// the wiki from showing content the model already produced.
    pub(super) async fn run(
        &self,
        n: usize,
        job: PageJob,
        pb: &PageBar,
        writer: Option<&PageWriter>,
        run_id: &str,
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
            pb.done(format!(
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

        pb.note(format!("撰页 {n}/{total} {}", page.rel_path));

        if !self.use_llm {
            // Every page of a key-less run is a template, so it counts as "no
            // model output" in the run summary.
            self.counters.fail.fetch_add(1, Ordering::Relaxed);
            // Keep whatever the last real run produced: a key-less run must not
            // replace model-written pages with structural templates.
            if keep_body_on_failure {
                return self.keep_previous(&page, &fingerprint, None, "模板模式", pb, n, started);
            }
            pb.done(format!("✓ {n}/{total} {} · 模板", page.rel_path));
            return self.templated(page, fingerprint, None);
        }

        // The page file is opened before the first token and grows as the model
        // writes: the wiki never waits for a long page to be finished, and a run
        // that dies keeps the text that had already arrived.
        let draft = writer.and_then(|w| match w.begin_draft(&page) {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::warn!("[{n}/{total}] {} 无法写入草稿: {e:#}", page.rel_path);
                None
            }
        });

        let label = format!("撰页 {n}/{total} {}", page.rel_path);
        let sink = match &draft {
            Some(d) => {
                let draft_sink: Arc<dyn atlas_llm::StreamSink> = d.clone();
                atlas_llm::sink_pair(pb.clone().stream_sink(label), draft_sink)
            }
            None => pb.clone().stream_sink(label),
        };

        match atlas_llm::with_stream_sink(
            sink,
            generate_page_with_llm(&self.llm, &evidence, &page, &self.cfg, &self.tools),
        )
        .await
        {
            Ok((usage, body)) if body.trim().chars().count() < 80 => {
                self.counters.fail.fetch_add(1, Ordering::Relaxed);
                if let Some(d) = &draft {
                    d.restore();
                }
                if keep_body_on_failure {
                    return self.keep_previous(
                        &page,
                        &fingerprint,
                        Some(usage),
                        "输出过短",
                        pb,
                        n,
                        started,
                    );
                }
                pb.done(format!(
                    "~ {n}/{total} {} · 模板回退 · {:.1}s",
                    page.rel_path,
                    started.elapsed().as_secs_f64()
                ));
                self.templated(page, fingerprint, Some(usage))
            }
            Ok((usage, body)) => {
                // Land the first draft before any depth rewrite so the reader
                // (and the index) see content as soon as the model finishes.
                // usage=None: the final write reports the accumulated totals.
                // The handle is closed first: the write replaces the file.
                if let Some(d) = &draft {
                    d.close();
                }
                if let Some(writer) = writer {
                    let first = GeneratedPageWithMeta {
                        page: page.clone(),
                        body: body.clone(),
                        usage: None,
                        fingerprint: fingerprint.clone(),
                        outcome: PageOutcome::Generated,
                    };
                    if let Err(e) = writer.write(&self.llm, run_id, n, first).await {
                        tracing::warn!("[{n}/{total}] {} 首稿落盘失败: {e:#}", page.rel_path);
                    }
                }
                let (body, usage, expanded) = if self.cfg.llm.depth_pass {
                    // The rewrite goes to memory, not to the file: reopening the
                    // draft would truncate the first draft that just landed, and
                    // a run killed mid-rewrite would lose it. The reader keeps
                    // the complete first draft until the revision replaces it.
                    deepen_page(
                        &self.llm,
                        &self.tools,
                        &self.cfg,
                        &page,
                        &evidence,
                        body,
                        usage,
                        pb,
                        n,
                        total,
                    )
                    .await
                } else {
                    (body, usage, false)
                };
                if let Some(d) = &draft {
                    d.close();
                }
                if expanded {
                    self.counters.expanded.fetch_add(1, Ordering::Relaxed);
                }
                self.counters.ok.fetch_add(1, Ordering::Relaxed);
                pb.done(format!(
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
                if let Some(d) = &draft {
                    d.restore();
                }
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
                        pb,
                        n,
                        started,
                    );
                }
                pb.done(format!(
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
        pb: &PageBar,
        n: usize,
        started: std::time::Instant,
    ) -> GeneratedPageWithMeta {
        self.counters.kept.fetch_add(1, Ordering::Relaxed);
        pb.done(format!(
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
    call_model(llm, tools, cfg.llm.max_tool_rounds, &system, &user).await
}

/// Pages whose body came from the structural template are *not* up to date:
/// poisoning the stored fingerprint makes the next `atlas update` ask the model
/// again instead of silently reusing template text (which is what happened after
/// a failed LLM run or a key-less template run).
fn template_fingerprint(fingerprint: &str) -> String {
    format!("{fingerprint}+template")
}
