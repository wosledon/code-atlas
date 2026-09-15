use super::*;

/// One planned page together with everything the generation stage needs:
/// the evidence prompt, its fingerprint and (when nothing changed) the body
/// produced by an earlier run.
pub(crate) struct PageJob {
    pub(crate) page: PlannedPage,
    pub(crate) evidence: String,
    pub(crate) fingerprint: String,
    pub(crate) reuse_body: Option<String>,
    /// A real (LLM-written) body already exists on disk for this page, so a
    /// failed regeneration must keep it instead of replacing it with a
    /// structural template.
    pub(crate) keep_body_on_failure: bool,
}

/// How a page was produced. It decides whether the write phase replaces the
/// file on disk at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageOutcome {
    /// Fresh body written by the model.
    Generated,
    /// Previous body reused because the evidence (and the prompts) did not change.
    Reused,
    /// Structural template, used when no LLM is configured.
    Template,
    /// Generation failed and the previous body was left untouched on disk.
    KeptPrevious,
}

/// One generated page plus the bookkeeping the write phase needs. Pages travel
/// from the generation stage to the writer one at a time, so a run that is
/// interrupted keeps every page it already produced.
pub(crate) struct GeneratedPageWithMeta {
    pub(crate) page: PlannedPage,
    pub(crate) body: String,
    pub(crate) usage: Option<(i64, i64, i64)>,
    pub(crate) fingerprint: String,
    pub(crate) outcome: PageOutcome,
}

/// What persisting one page did. Produced by the page's own task and merged
/// into the run totals by the caller, so the summary never depends on the order
/// in which the pages happened to finish.
pub(crate) struct PageReport {
    pub(crate) rel_path: String,
    /// `(prompt_tokens, completion_tokens, latency_ms)` spent on this page.
    pub(crate) usage: Option<(i64, i64, i64)>,
    pub(crate) outcome: PageOutcome,
    /// Chunks now stored for the page.
    pub(crate) chunks: usize,
    /// The stored split was still current, so no re-chunking was needed.
    pub(crate) reused_chunks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct LastUpdate {
    pub updatedAt: String,
    pub command: String,
    pub gitHead: Option<String>,
    pub model: Option<String>,
    pub runId: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run_id: String,
    pub mode: String,
    pub status: String,
    pub pages_written: Vec<String>,
    pub entities: usize,
    pub chunks: usize,
    pub no_op: bool,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub notes: Vec<String>,
}

pub struct PipelineCtx {
    pub repo_root: PathBuf,
    pub cfg: AtlasConfig,
}
