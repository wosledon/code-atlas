use super::*;

/// One planned page together with everything the generation stage needs:
/// the evidence prompt, its fingerprint and (when nothing changed) the body
/// produced by an earlier run.
pub(crate) struct PageJob {
    pub(crate) page: PlannedPage,
    pub(crate) evidence: String,
    pub(crate) fingerprint: String,
    pub(crate) reuse_body: Option<String>,
}

/// Output of the generation stage: `(plan index, page, body, usage, fingerprint, reused)`.
pub(crate) type GeneratedPage = (PlannedPage, String, Option<(i64, i64, i64)>, String, bool);

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
