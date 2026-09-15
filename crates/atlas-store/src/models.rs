use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub mode: String,
    pub status: String,
    pub git_head: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub language: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub est_cost_usd: f64,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub canonical_key: String,
    pub props: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRow {
    pub id: String,
    pub page_path: String,
    pub ord: i64,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub start_line: i64,
    pub end_line: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub page_path: Option<String>,
    pub score: f64,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub start_line: Option<i64>,
    #[serde(default)]
    pub end_line: Option<i64>,
}

impl SearchHit {
    pub fn entity(kind: String, id: String, title: String, summary: String) -> Self {
        Self {
            kind,
            id,
            title,
            summary,
            page_path: None,
            score: 1.0,
            body: None,
            start_line: None,
            end_line: None,
        }
    }
}

/// Which retrieval backend `Store::search` should use. `kb.search` in
/// `atlas.toml` maps onto this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Prefer FTS5, fall back to LIKE (default).
    Auto,
    /// Same as Auto but logs when FTS5 is unavailable.
    Fts5,
    /// Keyword `LIKE` scan only.
    Like,
}

impl SearchMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "like" | "like-scan" => Self::Like,
            "fts5" | "fts" => Self::Fts5,
            _ => Self::Auto,
        }
    }

    pub(crate) fn allows_fts(self) -> bool {
        !matches!(self, Self::Like)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRow {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub description: String,
    pub body_hash: String,
    pub evidence_hash: String,
}

pub fn body_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    hex::encode(h.finalize())
}
