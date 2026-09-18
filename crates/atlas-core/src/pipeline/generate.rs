//! Fan-out of the generation stage: every planned page is produced **and
//! persisted** by its own task (bounded by the configured concurrency), so the
//! wiki, the index and the chunks land as soon as that page is ready instead of
//! waiting for the whole run. This module owns the task set and the run summary;
//! [`super::pagegen`] produces one body and [`super::write`] stores it.

use super::pagegen::PageGen;
use super::progress::Progress;
use super::run::{RunContext, RunCounts};
use super::types::{PageJob, PageOutcome, PageReport};
use super::write::PageWriter;
use super::*;

use anyhow::anyhow;
use std::sync::Arc;

/// Render the planned pages concurrently; each task writes its page the moment
/// the body exists. Pages whose evidence is unchanged keep their previous body;
/// a page the model fails to produce keeps its previous version when it has one
/// (a failed run must never destroy a good page) and only falls back to the
/// structural template when there is nothing to keep.
pub(super) async fn generate_pages(
    run: &RunContext<'_>,
    jobs: Vec<PageJob>,
    counts: &mut RunCounts,
) -> Result<()> {
    let ctx = run.ctx;
    let session = run.session;
    let llm = run.llm.clone();
    let config = Arc::new(ctx.cfg.clone());
    let use_llm = run.use_llm;
    let conc = ctx.cfg.llm.concurrency.max(1);
    let tools = Arc::new(RepoTools::new(
        &ctx.repo_root,
        ctx.cfg.privacy.redact_paths.clone(),
        ctx.cfg.privacy.max_file_bytes,
    )?);
    let total_pages = run.plan.len() as u64;
    let reuse_planned = jobs.iter().filter(|j| j.reuse_body.is_some()).count();

    // Printed (not logged) on purpose: the progress bars own stderr, and a log
    // line would double up with this one on the CLI.
    println!(
        "[atlas] 规划完成：共 {total_pages} 页 · 并发 {conc} · {}/{} · llm={}",
        ctx.cfg.llm.provider,
        ctx.cfg.llm.model,
        if use_llm { "on" } else { "off (template)" }
    );

    let progress = Progress::new(
        total_pages,
        format!(
            "并发 {conc} · {}",
            if use_llm {
                format!("{}/{}", ctx.cfg.llm.provider, ctx.cfg.llm.model)
            } else {
                "模板模式".into()
            }
        ),
    );
    progress.line(
        "逐页落盘：每页生成完成即写入 atlas/ 与索引（长运行可实时看到进度，中断不丢已完成页）",
    );

    let writer = PageWriter::new(
        session.store.clone(),
        config.clone(),
        session.atlas_root.clone(),
        use_llm,
        progress.clone(),
    );
    let generator = PageGen::new(
        llm.clone(),
        run.scan.clone(),
        config,
        tools.clone(),
        use_llm,
        total_pages,
    );
    let run_id = session.run_id.clone();

    // Pages run as independent tasks so generating the next page overlaps
    // chunking and writing the previous one; the semaphore bounds how many pages
    // are in flight (and therefore how many model calls run at once).
    let permits = Arc::new(tokio::sync::Semaphore::new(conc));
    let mut tasks = tokio::task::JoinSet::new();
    for (idx, job) in jobs.into_iter().enumerate() {
        let generator = generator.clone();
        let writer = writer.clone();
        let progress = progress.clone();
        let permits = permits.clone();
        let llm = llm.clone();
        let run_id = run_id.clone();
        tasks.spawn(async move {
            let _permit = permits
                .acquire_owned()
                .await
                .expect("page semaphore is never closed");
            let n = idx + 1;
            let pb = progress.page(n, &job.page.rel_path);
            let produced = generator.run(n, job, &pb, Some(&writer), &run_id).await;
            writer.write(&llm, &run_id, n, produced).await
        });
    }

    let mut finished = 0usize;
    while let Some(joined) = tasks.join_next().await {
        let report = match joined {
            Ok(Ok(report)) => report,
            Ok(Err(e)) => {
                tasks.abort_all();
                return Err(e);
            }
            Err(e) => {
                tasks.abort_all();
                return Err(anyhow!("page task failed: {e}"));
            }
        };
        finished += 1;
        merge_page(counts, report);
        progress.generated(finished, generator.ok(), generator.failed());
    }

    report_run(
        &generator,
        &progress,
        total_pages,
        reuse_planned,
        counts,
        use_llm,
        Caches {
            tools: tools.cache_stats(),
            prompt: llm.prompt_cache_totals(),
        },
    );
    Ok(())
}

/// Fold one page's result into the run totals. Merging here (instead of inside
/// the page tasks) keeps the numbers independent of the order the pages finish
/// in and gives the summary a single owner.
fn merge_page(counts: &mut RunCounts, report: PageReport) {
    if let Some((pt, ct, _)) = report.usage {
        counts.prompt_tokens += pt;
        counts.completion_tokens += ct;
    }
    match report.outcome {
        PageOutcome::Reused => counts.pages_reused += 1,
        PageOutcome::Template => counts.template_pages += 1,
        PageOutcome::KeptPrevious => {
            counts.pages_kept += 1;
            counts.kept_pages.push(report.rel_path.clone());
        }
        PageOutcome::Generated => {}
    }
    if report.outcome != PageOutcome::KeptPrevious {
        counts.pages_written.push(report.rel_path.clone());
        counts.chunks += report.chunks;
        if report.reused_chunks {
            counts.reused_chunks += 1;
        }
    }
}

/// What the run's two caches did. Reported together because both answer the
/// same question — how much of this run was paid for twice.
#[derive(Clone, Copy, Default)]
struct Caches {
    /// Tool results served from the run-scoped snapshot cache.
    tools: CacheStats,
    /// `(prompt tokens, of which served from the provider's prompt cache)`.
    prompt: (i64, i64),
}

/// Close the bars and report what the run did, including the two situations the
/// operator has to act on: pages the depth gate rewrote and pages the model
/// failed to produce.
fn report_run(
    generator: &PageGen,
    progress: &Progress,
    total_pages: u64,
    reuse_planned: usize,
    counts: &mut RunCounts,
    use_llm: bool,
    caches: Caches,
) {
    let (ok, failed, expanded, kept) = (
        generator.ok(),
        generator.failed(),
        generator.expanded(),
        generator.kept(),
    );
    progress.finish(
        format!(
            "生成结束 · 成功 {ok} / 失败 {failed} / 深度重写 {expanded} / 复用 {reuse_planned} / 共 {total_pages}"
        ),
        format!(
            "已落盘 {} 页 · 复用页 {} · 模板 {} · chunks {} · 复用分块 {}",
            counts.pages_written.len(),
            counts.pages_reused,
            counts.template_pages,
            counts.chunks,
            counts.reused_chunks
        ),
    );

    let cache = caches.tools;
    if cache.lookups() > 0 {
        counts.notes.push(format!(
            "tool cache: {}/{} lookups hit ({:.0}%), {} held",
            cache.hits,
            cache.lookups(),
            cache.hit_rate() * 100.0,
            cache.held()
        ));
        progress.line(&format!(
            "工具读取缓存：命中 {}/{}（{:.0}%）· 缓存 {}（同一文件被多页读到时不重复读盘）",
            cache.hits,
            cache.lookups(),
            cache.hit_rate() * 100.0,
            cache.held()
        ));
    }
    let (prompt_tokens, cached_tokens) = caches.prompt;
    if cached_tokens > 0 && prompt_tokens > 0 {
        let pct = cached_tokens as f64 / prompt_tokens as f64 * 100.0;
        counts.notes.push(format!(
            "prompt cache: {cached_tokens}/{prompt_tokens} prompt tokens served from cache ({pct:.0}%)"
        ));
        progress.line(&format!(
            "提示词缓存：{cached_tokens}/{prompt_tokens} prompt tokens 命中供应商缓存（{pct:.0}%）· 稳定前缀（系统提示 + 仓库共享证据）在最前"
        ));
    } else if prompt_tokens > 0 {
        counts
            .notes
            .push("prompt cache: provider reported no cached prompt tokens this run".to_string());
    }
    if !counts.kept_pages.is_empty() {
        counts.notes.push(format!(
            "kept the previous body of {} page(s) whose regeneration failed: {}",
            counts.kept_pages.len(),
            counts.kept_pages.join(", ")
        ));
        progress.line(&format!(
            "{} 页生成失败，已保留上一版正文（下次 update 会自动重试）：{}",
            counts.kept_pages.len(),
            counts.kept_pages.join(" / ")
        ));
    }
    if expanded > 0 {
        counts.notes.push(format!(
            "depth gate rewrote {expanded}/{total_pages} pages in a second pass"
        ));
        progress.line(&format!(
            "深度门重写了 {expanded} 页（第一稿过浅，已二次扩写）"
        ));
    }
    if use_llm && failed > kept {
        let first = generator
            .first_error()
            .unwrap_or_else(|| "unknown error".into());
        counts.notes.push(format!(
            "problem: {failed} pages failed to generate; first error: {first}"
        ));
        progress.line(&format!(
            "警告：{failed} 页生成失败（{kept} 页保留上一版正文，{} 页回退模板）· 首个错误：{first}",
            failed - kept
        ));
        progress.line(
            "这些页不会被记为最新版本：修好原因后重跑 `atlas update` 会自动重试（已落盘的其他页会被复用）",
        );
    }
}
