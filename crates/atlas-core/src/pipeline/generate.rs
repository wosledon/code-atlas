use super::run::{GeneratedPageWithMeta, RunContext, RunCounts};
use super::template::template_page_body;
use super::types::{GeneratedPage, PageJob};
use super::*;

use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) async fn generate_page_with_llm(
    llm: &LlmClient,
    evidence: &str,
    page: &PlannedPage,
    cfg: &AtlasConfig,
    tools: &RepoTools,
) -> Result<((i64, i64, i64), String)> {
    let system = format!(
        "You are Code Atlas, a senior engineer writing an in-repo knowledge wiki that explains \
         an existing codebase so a new maintainer can take over quickly. The wiki must describe the \
         WHOLE repository, not just one directory.\n\
         Output language: {}.\n\n\
         Built-in requirements (always apply, regardless of project type):\n\
         1) Audience: experienced developer on day-one of maintaining THIS repository.\n\
         2) Ground every claim in real evidence: read the actual files with the provided tools \
         (read_file / list_files / grep) before describing behaviour, APIs, tables, routes, config \
         keys or workflows. Never invent symbols or paths.\n\
         3) Cover: business/domain problem, core concepts → code mapping, architecture & boundaries, \
         data model, external interfaces, key flows, how to run/change safely.\n\
         4) Prefer tables (file maps, APIs, configs). Prefer mermaid (flowchart / sequenceDiagram / erDiagram / classDiagram) \
         when a diagram clarifies structure or flow.\n\
         5) Module pages: responsibility, key real paths, public entrypoints, inbound/outbound deps, pitfalls, \
         '上手要点' (what to read/change first).\n\
         6) End factual pages with ## Claims (short verifiable bullets).\n\
         7) Dense technical Chinese (or the output language). No marketing, no filler.\n\
         8) This is a CODEBASE EXPLANATION wiki, not a product marketing site.\n\
         9) Cite repository-relative paths in backticks so readers and search can jump to the code.",
        cfg.output.language
    );
    let user = format!(
        "Page title: {}\nSection type: {}\nFocus: {}\n\n\
         Inspect the real code with the tools (paths are repository-relative), then write the full \
         markdown body only (no YAML front matter). Prefer depth over brevity.\n\n\
         Repository map:\n{evidence}",
        page.title, page.page_type, page.focus
    );
    let rounds = cfg.llm.max_tool_rounds;
    let resp = if rounds > 0 && llm.supports_tools() {
        let specs = RepoTools::specs();
        llm.chat_with_tools(&system, &user, &specs, rounds, |name, args| {
            tools.call(name, args)
        })
        .await?
    } else {
        llm.chat(&system, &user).await?
    };
    let usage = (
        resp.usage.prompt_tokens,
        resp.usage.completion_tokens,
        resp.usage.latency_ms,
    );
    Ok((usage, resp.text))
}

/// Render every planned page concurrently with live progress reporting. Pages
/// whose evidence is unchanged keep their previous body; anything the LLM fails
/// to produce (or produces too briefly) falls back to the structural template.
pub(super) async fn generate_pages(
    run: &RunContext<'_>,
    jobs: Vec<PageJob>,
    counts: &mut RunCounts,
) -> Result<Vec<GeneratedPageWithMeta>> {
    let ctx = run.ctx;
    let llm = run.llm.clone();
    let scan = run.scan.clone();
    let cfg = Arc::new(ctx.cfg.clone());
    let use_llm = run.use_llm;
    let conc = ctx.cfg.llm.concurrency.max(1);
    let tools = Arc::new(RepoTools::new(
        &ctx.repo_root,
        ctx.cfg.privacy.redact_paths.clone(),
        ctx.cfg.privacy.max_file_bytes,
    )?);
    let total_pages = run.plan.len() as u64;
    let reuse_planned = jobs.iter().filter(|j| j.reuse_body.is_some()).count();

    let mp = MultiProgress::new();
    let overall = mp.add(ProgressBar::new(total_pages));
    overall.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} 总进度 {pos}/{len} {bar:40.cyan/blue} {msg}",
        )
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

    tracing::info!(
        "规划完成：共 {total_pages} 页，并发 {conc}，provider={} model={} llm={}",
        ctx.cfg.llm.provider,
        ctx.cfg.llm.model,
        if use_llm { "on" } else { "off (template)" }
    );

    let done = Arc::new(AtomicUsize::new(0));
    let ok = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicUsize::new(0));
    let first_llm_error: Arc<Mutex<Option<String>>> = Default::default();

    let mut generated: Vec<GeneratedPageWithMeta> = stream::iter(jobs.into_iter().enumerate())
        .map(|(idx, job)| {
            let llm = llm.clone();
            let scan = scan.clone();
            let cfg = cfg.clone();
            let tools = tools.clone();
            let done = done.clone();
            let ok = ok.clone();
            let fail = fail.clone();
            let overall = overall.clone();
            let mp = mp.clone();
            let first_llm_error = first_llm_error.clone();
            let PageJob {
                page,
                evidence,
                fingerprint,
                reuse_body,
            } = job;
            async move {
                let n = idx + 1;
                let sp = mp.add(ProgressBar::new_spinner());
                sp.set_style(
                    ProgressStyle::with_template("{spinner:.yellow} {msg}")
                        .unwrap_or_else(|_| ProgressStyle::default_spinner()),
                );
                sp.enable_steady_tick(std::time::Duration::from_millis(100));

                let started = std::time::Instant::now();
                let result: Result<GeneratedPage, anyhow::Error> = if let Some(body) = reuse_body {
                    sp.finish_with_message(format!(
                        "↻ {n}/{total_pages} {} · 复用（证据未变）",
                        page.rel_path
                    ));
                    ok.fetch_add(1, Ordering::Relaxed);
                    Ok((page, body, None, fingerprint, true))
                } else {
                    sp.set_message(format!("撰页 {n}/{total_pages} {}", page.rel_path));
                    tracing::info!("[{n}/{total_pages}] 生成 {}", page.rel_path);
                    if use_llm {
                        match generate_page_with_llm(&llm, &evidence, &page, &cfg, &tools).await {
                            Ok((usage, body)) => {
                                let (body, used) = if body.trim().chars().count() < 80 {
                                    sp.set_message(format!(
                                        "输出过短，改模板 {n}/{total_pages} {}",
                                        page.rel_path
                                    ));
                                    (template_page_body(&scan, &page, &cfg), false)
                                } else {
                                    (body, true)
                                };
                                if used {
                                    ok.fetch_add(1, Ordering::Relaxed);
                                    sp.finish_with_message(format!(
                                        "✓ {n}/{total_pages} {} · tokens {}+{} · {:.1}s",
                                        page.rel_path,
                                        usage.0,
                                        usage.1,
                                        usage.2 as f64 / 1000.0
                                    ));
                                } else {
                                    fail.fetch_add(1, Ordering::Relaxed);
                                    sp.finish_with_message(format!(
                                        "~ {n}/{total_pages} {} · 模板回退 · {:.1}s",
                                        page.rel_path,
                                        started.elapsed().as_secs_f64()
                                    ));
                                }
                                Ok((page, body, Some(usage), fingerprint, false))
                            }
                            Err(e) => {
                                fail.fetch_add(1, Ordering::Relaxed);
                                let msg = format!("{e:#}");
                                let short: String = msg.chars().take(80).collect();
                                if let Ok(mut slot) = first_llm_error.lock()
                                    && slot.is_none()
                                {
                                    *slot = Some(short.clone());
                                }
                                sp.finish_with_message(format!(
                                    "! {n}/{total_pages} {} · 回退模板 · {short}",
                                    page.rel_path
                                ));
                                tracing::warn!("[{n}/{total_pages}] {} 回退: {e:#}", page.rel_path);
                                Ok((
                                    page.clone(),
                                    template_page_body(&scan, &page, &cfg),
                                    None,
                                    fingerprint,
                                    false,
                                ))
                            }
                        }
                    } else {
                        sp.finish_with_message(format!(
                            "✓ {n}/{total_pages} {} · 模板",
                            page.rel_path
                        ));
                        Ok((
                            page.clone(),
                            template_page_body(&scan, &page, &cfg),
                            None,
                            fingerprint,
                            false,
                        ))
                    }
                };

                let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                overall.set_position(finished as u64);
                overall.set_message(format!(
                    "完成 {finished}/{total_pages} · 成功 {} · 回退 {}",
                    ok.load(Ordering::Relaxed),
                    fail.load(Ordering::Relaxed)
                ));
                result.map(|(p, b, u, f, r)| (idx, p, b, u, f, r))
            }
        })
        .buffer_unordered(conc)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.expect("page generation never fails"))
        .collect();
    generated.sort_by_key(|(idx, ..)| *idx);

    overall.finish_with_message(format!(
        "LLM 阶段结束 · 成功 {} / 回退 {} / 复用 {reuse_planned} / 共 {total_pages}",
        ok.load(Ordering::Relaxed),
        fail.load(Ordering::Relaxed)
    ));
    println!("[atlas] 开始落盘与索引（{} 页）…", generated.len());

    let fail_n = fail.load(Ordering::Relaxed);
    if use_llm && fail_n > 0 {
        let first = first_llm_error
            .lock()
            .ok()
            .and_then(|s| s.clone())
            .unwrap_or_else(|| "unknown error".into());
        counts.notes.push(format!(
            "problem: {fail_n} pages fell back to templates; first error: {first}"
        ));
        println!("[atlas] 警告：{fail_n} 页 LLM 生成失败，已回退模板（{first}）");
    }
    Ok(generated)
}
