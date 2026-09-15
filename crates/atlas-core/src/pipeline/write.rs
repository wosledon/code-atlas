use super::progress::Progress;
use super::types::{GeneratedPageWithMeta, PageOutcome, PageReport};
use super::*;

use std::sync::Arc;

/// Label stored on chunk rows: the splitter used plus its signature (mode,
/// target size, whether the LLM was available) so a config change invalidates
/// the existing chunks instead of silently keeping the old split.
pub(super) fn chunk_source_label(mode: &str, target_tokens: usize, use_llm: bool) -> String {
    format!("{mode}|{target_tokens}|llm={use_llm}")
}

/// Persists finished pages **from the task that generated them**: the model
/// output reaches `atlas/`, the index and the chunks as soon as it exists, so a
/// long run is visible while it works and an interrupted or killed run keeps
/// every page it already paid for. Cloned once per page task; the writer holds
/// no per-run mutable state.
#[derive(Clone)]
pub(super) struct PageWriter {
    store: Arc<Store>,
    config: Arc<AtlasConfig>,
    atlas_root: PathBuf,
    use_llm: bool,
    chunk_label: String,
    progress: Progress,
}

impl PageWriter {
    pub(super) fn new(
        store: Arc<Store>,
        config: Arc<AtlasConfig>,
        atlas_root: PathBuf,
        use_llm: bool,
        progress: Progress,
    ) -> Self {
        let chunk_label = chunk_source_label(
            &config.kb.chunk.mode,
            config.kb.chunk.target_tokens,
            use_llm,
        );
        Self {
            store,
            config,
            atlas_root,
            use_llm,
            chunk_label,
            progress,
        }
    }

    /// Write one page: front matter + body, claims, and (only when the body
    /// actually changed) its chunks. Reused pages keep their chunks and FTS rows.
    pub(super) async fn write(
        &self,
        llm: &LlmClient,
        run_id: &str,
        n: usize,
        produced: GeneratedPageWithMeta,
    ) -> Result<PageReport> {
        let store = self.store.as_ref();
        let atlas_root = &self.atlas_root;
        let use_llm = self.use_llm;
        let GeneratedPageWithMeta {
            page,
            body,
            usage,
            fingerprint,
            outcome,
        } = produced;

        if let Some((pt, ct, lat)) = usage {
            store.record_llm_call(run_id, "page", llm.provider(), llm.model(), pt, ct, lat, "ok")?;
        }
        let mut report = PageReport {
            rel_path: page.rel_path.clone(),
            usage,
            outcome,
            chunks: 0,
            reused_chunks: false,
        };
        // The model failed for this page, but the previous version is still the
        // best thing we have: leave the file, its claims and its chunks
        // untouched and let the next run retry the fingerprint.
        if outcome == PageOutcome::KeptPrevious {
            self.progress
                .landed(n, &page.rel_path, " · 生成失败，保留上一版");
            return Ok(report);
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
            report.reused_chunks = true;
            report.chunks = existing_chunks as usize;
            if let Some(m) = page.module.as_deref() {
                link_page_to_module(store, run_id, &page.rel_path, m)?;
            }
            self.progress.landed(
                n,
                &page.rel_path,
                &format!(" · 未变化 · chunks {existing_chunks} 复用"),
            );
            return Ok(report);
        }

        let mode_c = ChunkMode::parse(&self.config.kb.chunk.mode);
        let target_tokens = self.config.kb.chunk.target_tokens;
        let (specs, source) = match mode_c {
            ChunkMode::Structural => (atlas_kb::structural_chunks(&body, target_tokens), "structural"),
            _ => {
                if use_llm {
                    match atlas_kb::semantic_chunk_page(llm, &page.title, &body, target_tokens).await
                    {
                        Ok((specs, from_llm)) => {
                            (specs, if from_llm { "llm-semantic" } else { "structural" })
                        }
                        Err(e) => {
                            tracing::warn!("chunk fallback {}: {e:#}", page.rel_path);
                            (
                                atlas_kb::structural_chunks(&body, target_tokens),
                                "structural",
                            )
                        }
                    }
                } else {
                    (
                        atlas_kb::structural_chunks(&body, target_tokens),
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
        report.chunks = store.count_chunks_for_page(&page.rel_path)? as usize;
        if let Some(m) = page.module.as_deref() {
            link_page_to_module(store, run_id, &page.rel_path, m)?;
        }
        self.progress.landed(
            n,
            &page.rel_path,
            &format!(" · chunks {}（{source}）", report.chunks),
        );
        Ok(report)
    }
}

