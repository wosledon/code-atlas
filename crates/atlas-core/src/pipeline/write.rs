use super::run::{RunContext, RunCounts};
use super::types::{GeneratedPageWithMeta, PageOutcome};
use super::*;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Label stored on chunk rows: the splitter used plus its signature (mode,
/// target size, whether the LLM was available) so a config change invalidates
/// the existing chunks instead of silently keeping the old split.
pub(super) fn chunk_source_label(mode: &str, target_tokens: usize, use_llm: bool) -> String {
    format!("{mode}|{target_tokens}|llm={use_llm}")
}

/// Persists finished pages **one at a time, while the rest are still being
/// generated**: the model output reaches `atlas/` and the index as soon as it
/// exists, so a long run is visible while it works and an interrupted or killed
/// run keeps every page it already paid for.
pub(super) struct PageWriter {
    write_pb: ProgressBar,
    chunk_label: String,
    total_chunks: usize,
    kept: Vec<String>,
}

impl PageWriter {
    pub(super) fn new(
        mp: &MultiProgress,
        cfg: &AtlasConfig,
        use_llm: bool,
        total_pages: u64,
    ) -> Self {
        let write_pb = mp.add(ProgressBar::new(total_pages));
        write_pb.set_style(
            ProgressStyle::with_template("{spinner:.green} 落盘 {pos}/{len} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        write_pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Self {
            write_pb,
            chunk_label: chunk_source_label(
                &cfg.kb.chunk.mode,
                cfg.kb.chunk.target_tokens,
                use_llm,
            ),
            total_chunks: 0,
            kept: Vec::new(),
        }
    }

    /// Write one page: front matter + body, claims, and (only when the body
    /// actually changed) its chunks. Reused pages keep their chunks and FTS rows.
    pub(super) async fn write(
        &mut self,
        run: &RunContext<'_>,
        produced: GeneratedPageWithMeta,
        counts: &mut RunCounts,
    ) -> Result<()> {
        let ctx = run.ctx;
        let session = run.session;
        let store = &session.store;
        let run_id = session.run_id.as_str();
        let atlas_root = &session.atlas_root;
        let llm = run.llm;
        let use_llm = run.use_llm;
        let GeneratedPageWithMeta {
            page,
            body,
            usage,
            fingerprint,
            outcome,
        } = produced;

        if let Some((pt, ct, lat)) = usage {
            counts.prompt_tokens += pt;
            counts.completion_tokens += ct;
            store.record_llm_call(run_id, "page", llm.provider(), llm.model(), pt, ct, lat, "ok")?;
        }
        match outcome {
            PageOutcome::Reused => counts.pages_reused += 1,
            PageOutcome::Template => counts.template_pages += 1,
            // The model failed for this page, but the previous version is still
            // the best thing we have: leave the file, its claims and its chunks
            // untouched and let the next run retry the fingerprint.
            PageOutcome::KeptPrevious => {
                counts.pages_kept += 1;
                self.kept.push(page.rel_path.clone());
                self.write_pb.inc(1);
                self.write_pb
                    .set_message(format!("{} · 生成失败，保留上一版", page.rel_path));
                return Ok(());
            }
            PageOutcome::Generated => {}
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

        // Re-chunk only when the body actually changed *and* the existing chunks
        // came from the current chunker; otherwise the split is rebuilt.
        let existing_chunks = store.count_chunks_for_page(&page.rel_path)?;
        let chunks_current = store.count_chunks_for_page_with_signature(
            &page.rel_path,
            &self.chunk_label,
        )? == existing_chunks;
        let unchanged = prev.as_ref().is_some_and(|p| p.body_hash == hash);
        if unchanged && existing_chunks > 0 && chunks_current {
            counts.reused_chunks += 1;
            self.total_chunks += existing_chunks as usize;
            if let Some(m) = page.module.as_deref() {
                link_page_to_module(store, run_id, &page.rel_path, m)?;
            }
            counts.pages_written.push(page.rel_path.clone());
            self.write_pb.inc(1);
            self.write_pb.set_message(page.rel_path.clone());
            return Ok(());
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
        store_chunks(
            store,
            &page.rel_path,
            run_id,
            &atlas_kb::merge_small_chunks(specs),
            &format!("{source}|{}", self.chunk_label),
        )?;
        self.total_chunks += store.count_chunks_for_page(&page.rel_path)? as usize;
        if let Some(m) = page.module.as_deref() {
            link_page_to_module(store, run_id, &page.rel_path, m)?;
        }
        counts.pages_written.push(page.rel_path.clone());
        self.write_pb.inc(1);
        self.write_pb.set_message(page.rel_path.clone());
        Ok(())
    }

    /// Close the write progress bar and report what the run did with every page.
    pub(super) fn finish(&self, counts: &mut RunCounts) {
        self.write_pb.finish_with_message(format!(
            "已写入 {} 页 · 模板 {} · chunks={} · 复用页 {} · 复用分块 {}",
            counts.pages_written.len(),
            counts.template_pages,
            self.total_chunks,
            counts.pages_reused,
            counts.reused_chunks
        ));
        if !self.kept.is_empty() {
            counts.notes.push(format!(
                "kept the previous body of {} page(s) whose regeneration failed: {}",
                self.kept.len(),
                self.kept.join(", ")
            ));
            println!(
                "[atlas] {} 页生成失败，已保留上一版正文（下次 update 会自动重试）：{}",
                self.kept.len(),
                self.kept.join(" / ")
            );
        }
    }
}

