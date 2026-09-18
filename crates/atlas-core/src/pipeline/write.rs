use super::progress::Progress;
use super::types::{GeneratedPageWithMeta, PageOutcome, PageReport};
use super::*;

use atlas_llm::StreamSink;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Label stored on chunk rows: the splitter used plus its signature (mode,
/// target size, whether the LLM was available) so a config change invalidates
/// the existing chunks instead of silently keeping the old split.
pub(super) fn chunk_source_label(mode: &str, target_tokens: usize, use_llm: bool) -> String {
    format!("{mode}|{target_tokens}|llm={use_llm}")
}

/// Body of one page while the model is still writing it: front matter first,
/// then every streamed delta appended as it arrives, so `atlas/<page>.md` grows
/// on disk instead of appearing only when the page is finished.
///
/// The draft is not the page: it carries [`markdown::DRAFT_MARKER`], which makes
/// reuse and reindexing skip it, and a run that dies mid-page leaves it behind
/// as recoverable text that the next run overwrites.
pub(super) struct PageDraft {
    path: PathBuf,
    state: Mutex<DraftState>,
}

struct DraftState {
    /// `None` once closed: the final write replaces the file behind our back.
    file: Option<File>,
    /// Current length of the file; tracked to avoid a `metadata()` per delta.
    len: u64,
    /// Position of the innermost open round (see [`StreamSink::round_start`]).
    mark: u64,
    /// The file as it was before this run, put back when generation fails.
    previous: Option<Vec<u8>>,
}

impl PageWriter {
    /// Open the page file and start streaming into it. `None` when the writer
    /// has nothing to stream into (no LLM body is being written).
    pub(super) fn begin_draft(&self, page: &PlannedPage) -> Result<Arc<PageDraft>> {
        Ok(Arc::new(PageDraft::open(
            self.atlas_root.join(&page.rel_path),
            &FrontMatter::new(
                &page.page_type,
                &page.title,
                &page.description,
                &page.tags.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ),
        )?))
    }
}

impl PageDraft {
    pub(super) fn open(path: PathBuf, fm: &FrontMatter) -> Result<Self> {
        let previous = std::fs::read(&path)
            .ok()
            // A draft left by a killed run is not a page: never restore it.
            .filter(|bytes| !contains_marker(bytes));
        let draft = Self {
            path,
            state: Mutex::new(DraftState {
                file: None,
                len: 0,
                mark: 0,
                previous,
            }),
        };
        draft.open_file(fm)?;
        Ok(draft)
    }

    /// Truncate and write front matter + marker; the body follows as deltas.
    fn open_file(&self, fm: &FrontMatter) -> Result<()> {
        let mut state = self.lock();
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let header = format!("{}{}\n", fm.render(), markdown::DRAFT_MARKER);
        let mut file = File::create(&self.path)?;
        file.write_all(header.as_bytes())?;
        file.flush()?;
        state.len = header.len() as u64;
        state.mark = state.len;
        state.file = Some(file);
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DraftState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Close the file handle so the writer can replace it atomically.
    pub(super) fn close(&self) {
        self.lock().file = None;
    }

    /// Generation failed: put the page back the way this run found it.
    pub(super) fn restore(&self) {
        self.close();
        match self.lock().previous.take() {
            Some(bytes) => {
                if let Err(e) = markdown::atomic_write(&self.path, &bytes) {
                    tracing::warn!("恢复 {} 失败: {e:#}", self.path.display());
                }
            }
            None => {
                if let Err(e) = std::fs::remove_file(&self.path)
                    && e.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!("清理草稿 {} 失败: {e:#}", self.path.display());
                    }
            }
        }
    }

    fn append(&self, text: &str) {
        let mut state = self.lock();
        let Some(file) = state.file.as_mut() else {
            return;
        };
        if file.write_all(text.as_bytes()).is_ok() {
            state.len += text.len() as u64;
        }
    }

    fn start_mark(&self) {
        let mut state = self.lock();
        state.mark = state.len;
    }

    /// Drop everything written since the last mark: text a round produced before
    /// asking for a tool, or a partial attempt that is being retried.
    fn rollback(&self) {
        let mut state = self.lock();
        let mark = state.mark;
        if mark >= state.len {
            return;
        }
        let Some(file) = state.file.as_mut() else {
            return;
        };
        if file.set_len(mark).is_ok() {
            state.len = mark;
        }
    }
}

impl StreamSink for PageDraft {
    fn delta(&self, text: &str) {
        self.append(text);
    }

    fn round_start(&self) {
        self.start_mark();
    }

    fn round_end(&self, kept: bool) {
        if !kept {
            self.rollback();
        }
    }
}

fn contains_marker(bytes: &[u8]) -> bool {
    let needle = markdown::DRAFT_MARKER.as_bytes();
    bytes.windows(needle.len()).any(|w| w == needle)
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

