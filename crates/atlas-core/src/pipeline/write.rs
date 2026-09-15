use super::run::{GeneratedPageWithMeta, RunContext, RunCounts};
use super::*;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Persist the generated bodies, refresh their claims and re-chunk only the
/// pages whose body actually changed (unchanged pages keep their chunks and
/// FTS rows, which makes incremental runs cheap).
pub(super) async fn write_pages(
    run: &RunContext<'_>,
    generated: Vec<GeneratedPageWithMeta>,
    counts: &mut RunCounts,
) -> Result<()> {
    let ctx = run.ctx;
    let session = run.session;
    let store = &session.store;
    let run_id = session.run_id.as_str();
    let atlas_root = &session.atlas_root;
    let llm = run.llm;
    let use_llm = run.use_llm;

    let mp = MultiProgress::new();
    let write_pb = mp.add(ProgressBar::new(generated.len() as u64));
    write_pb.set_style(
        ProgressStyle::with_template("{spinner:.green} 落盘 {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    write_pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut total_chunks = 0usize;
    for (_idx, page, body, usage, fingerprint, was_reused) in generated {
        if was_reused {
            counts.pages_reused += 1;
        }
        if let Some((pt, ct, lat)) = usage {
            counts.prompt_tokens += pt;
            counts.completion_tokens += ct;
            store.record_llm_call(run_id, "page", llm.provider(), llm.model(), pt, ct, lat, "ok")?;
        }

        let fm = FrontMatter::new(
            &page.page_type,
            &page.title,
            &page.description,
            &page.tags.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        let abs = atlas_root.join(&page.rel_path);
        markdown::write_page(&abs, &fm, &body)?;
        let hash = markdown::hash_file(&abs);
        let prev = store.get_page(&page.rel_path)?;
        store.upsert_page(
            &page.rel_path,
            &page.title,
            &page.page_type,
            &page.description,
            &hash,
            &fingerprint,
            run_id,
        )?;

        let claims = claims_from_markdown(&page.rel_path, &body);
        for c in &claims {
            store.upsert_claim(
                &page.rel_path,
                &c.statement,
                &c.evidence.first().cloned().unwrap_or_default(),
                &c.status,
                run_id,
            )?;
        }
        write_page_claims(atlas_root, &page.rel_path, &claims)?;

        // Re-chunk only when the body actually changed; otherwise the existing
        // chunks (and their FTS rows) stay valid.
        let existing_chunks = store.count_chunks_for_page(&page.rel_path)?;
        let unchanged = prev.as_ref().is_some_and(|p| p.body_hash == hash);
        if unchanged && existing_chunks > 0 {
            counts.reused_chunks += 1;
            total_chunks += existing_chunks as usize;
            if let Some(m) = page.module.as_deref() {
                link_page_to_module(store, run_id, &page.rel_path, m)?;
            }
            counts.pages_written.push(page.rel_path.clone());
            write_pb.inc(1);
            write_pb.set_message(page.rel_path.clone());
            continue;
        }

        let mode_c = ChunkMode::parse(&ctx.cfg.kb.chunk.mode);
        let (specs, source) = match mode_c {
            ChunkMode::Structural => (
                atlas_kb::structural_chunks(&body, ctx.cfg.kb.chunk.target_tokens),
                "structural",
            ),
            _ => {
                if use_llm {
                    match atlas_kb::semantic_chunk_page(
                        llm,
                        &page.title,
                        &body,
                        ctx.cfg.kb.chunk.target_tokens,
                    )
                    .await
                    {
                        Ok((specs, from_llm)) => {
                            (specs, if from_llm { "llm-semantic" } else { "structural" })
                        }
                        Err(e) => {
                            tracing::warn!("chunk fallback {}: {e:#}", page.rel_path);
                            (
                                atlas_kb::structural_chunks(&body, ctx.cfg.kb.chunk.target_tokens),
                                "structural",
                            )
                        }
                    }
                } else {
                    (
                        atlas_kb::structural_chunks(&body, ctx.cfg.kb.chunk.target_tokens),
                        "structural",
                    )
                }
            }
        };
        store_chunks(store, &page.rel_path, run_id, &atlas_kb::merge_small_chunks(specs), source)?;
        total_chunks += store.count_chunks_for_page(&page.rel_path)? as usize;
        if let Some(m) = page.module.as_deref() {
            link_page_to_module(store, run_id, &page.rel_path, m)?;
        }
        counts.pages_written.push(page.rel_path.clone());
        write_pb.inc(1);
        write_pb.set_message(page.rel_path.clone());
    }
    write_pb.finish_with_message(format!(
        "已写入 {} 页 · chunks={total_chunks} · 复用页 {} · 复用分块 {}",
        counts.pages_written.len(),
        counts.pages_reused,
        counts.reused_chunks
    ));
    Ok(())
}
