//! Fan-out/fan-in of the generation stage: every planned page is produced by its
//! own task (bounded by the configured concurrency) and streamed into the
//! [`PageWriter`] as soon as it finishes, so a long run writes progress to disk
//! instead of keeping everything in memory until the end.

use super::pagegen::PageGen;
use super::run::{RunContext, RunCounts};
use super::types::PageJob;
use super::write::PageWriter;
use super::*;

use anyhow::anyhow;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Render the planned pages concurrently and hand every finished page straight
/// to the [`PageWriter`]. Pages whose evidence is unchanged keep their previous
/// body; a page the model fails to produce keeps its previous version when it
/// has one (a failed run must never destroy a good page) and only falls back to
/// the structural template when there is nothing to keep.
pub(super) async fn generate_pages(
    run: &RunContext<'_>,
    jobs: Vec<PageJob>,
    counts: &mut RunCounts,
) -> Result<()> {
    let ctx = run.ctx;
    let cfg = Arc::new(ctx.cfg.clone());
    let use_llm = run.use_llm;
    let conc = ctx.cfg.llm.concurrency.max(1);
    let tools = RepoTools::new(
        &ctx.repo_root,
        ctx.cfg.privacy.redact_paths.clone(),
        ctx.cfg.privacy.max_file_bytes,
    )?;
    let total_pages = run.plan.len() as u64;
    let reuse_planned = jobs.iter().filter(|j| j.reuse_body.is_some()).count();

    tracing::info!(
        "规划完成：共 {total_pages} 页，并发 {conc}，provider={} model={} llm={}",
        ctx.cfg.llm.provider,
        ctx.cfg.llm.model,
        if use_llm { "on" } else { "off (template)" }
    );

    let mp = MultiProgress::new();
    let overall = mp.add(ProgressBar::new(total_pages));
    overall.set_style(
        ProgressStyle::with_template("{spinner:.cyan} 总进度 {pos}/{len} {bar:40.cyan/blue} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    overall.set_message(format!(
        "并发 {conc} · {}",
        if use_llm {
            format!("{}/{}", ctx.cfg.llm.provider, ctx.cfg.llm.model)
        } else {
            "模板模式".into()
        }
    ));
    overall.enable_steady_tick(std::time::Duration::from_millis(80));
    mp.suspend(|| {
        println!(
            "[atlas] 逐页落盘：每页生成完成即写入 atlas/ 与索引（长运行可实时看到进度，中断不丢已完成页）"
        )
    });

    let mut writer = PageWriter::new(&mp, &ctx.cfg, use_llm, total_pages);
    let generator = PageGen::new(
        run.llm.clone(),
        run.scan.clone(),
        cfg,
        Arc::new(tools),
        use_llm,
        total_pages,
    );
    let done = Arc::new(AtomicUsize::new(0));

    // Pages run as independent tasks so they keep streaming while the main task
    // writes (and chunks) the previous page; the semaphore is what bounds the
    // number of pages in flight.
    let permits = Arc::new(tokio::sync::Semaphore::new(conc));
    let mut tasks = tokio::task::JoinSet::new();
    for (idx, job) in jobs.into_iter().enumerate() {
        let generator = generator.clone();
        let permits = permits.clone();
        let overall = overall.clone();
        let done = done.clone();
        let mp = mp.clone();
        tasks.spawn(async move {
            let _permit = permits
                .acquire_owned()
                .await
                .expect("page semaphore is never closed");
            let n = idx + 1;
            let sp = mp.add(ProgressBar::new_spinner());
            sp.set_style(
                ProgressStyle::with_template("{spinner:.yellow} {msg}")
                    .unwrap_or_else(|_| ProgressStyle::default_spinner()),
            );
            sp.enable_steady_tick(std::time::Duration::from_millis(100));

            let produced = generator.run(n, job, &sp).await;
            let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
            overall.set_position(finished as u64);
            overall.set_message(format!(
                "完成 {finished}/{total_pages} · 成功 {} · 失败 {}",
                generator.ok(),
                generator.failed()
            ));
            produced
        });
    }

    while let Some(joined) = tasks.join_next().await {
        let produced = match joined {
            Ok(produced) => produced,
            Err(e) => {
                tasks.abort_all();
                return Err(anyhow!("page generation task failed: {e}"));
            }
        };
        if let Err(e) = writer.write(run, produced, counts).await {
            tasks.abort_all();
            return Err(e);
        }
    }

    let (ok, fail_n, expanded, kept) = (
        generator.ok(),
        generator.failed(),
        generator.expanded(),
        generator.kept(),
    );
    overall.finish_with_message(format!(
        "生成结束 · 成功 {ok} / 失败 {fail_n} / 深度重写 {expanded} / 复用 {reuse_planned} / 共 {total_pages}"
    ));
    writer.finish(counts);

    if expanded > 0 {
        counts.notes.push(format!(
            "depth gate rewrote {expanded}/{total_pages} pages in a second pass"
        ));
        mp.suspend(|| println!("[atlas] 深度门重写了 {expanded} 页（第一稿过浅，已二次扩写）"));
    }
    if use_llm && fail_n > 0 {
        let first = generator.first_error().unwrap_or_else(|| "unknown error".into());
        counts.notes.push(format!(
            "problem: {fail_n} pages failed to generate; first error: {first}"
        ));
        mp.suspend(|| {
            println!(
                "[atlas] 警告：{fail_n} 页生成失败（{kept} 页保留上一版正文，{} 页回退模板）· 首个错误：{first}",
                fail_n - kept
            );
            println!(
                "[atlas] 这些页不会被记为最新版本：修好原因后重跑 `atlas update` 会自动重试（已落盘的其他页会被复用）"
            );
        });
    }
    Ok(())
}
