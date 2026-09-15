/// Columns added after V1 shipped; applied with `ALTER TABLE` and tolerated
/// when already present.
pub const MIGRATION_PAGE_EVIDENCE_HASH: &str =
    "ALTER TABLE pages ADD COLUMN evidence_hash TEXT NOT NULL DEFAULT ''";

pub const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS runs (
  id TEXT PRIMARY KEY,
  mode TEXT NOT NULL,
  status TEXT NOT NULL,
  git_head TEXT,
  provider TEXT,
  model TEXT,
  language TEXT,
  prompt_tokens INTEGER NOT NULL DEFAULT 0,
  completion_tokens INTEGER NOT NULL DEFAULT 0,
  est_cost_usd REAL NOT NULL DEFAULT 0,
  error TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);

CREATE TABLE IF NOT EXISTS llm_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  stage TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL DEFAULT 0,
  completion_tokens INTEGER NOT NULL DEFAULT 0,
  latency_ms INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pages (
  path TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  type TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  body_hash TEXT NOT NULL,
  run_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS claims (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  page_path TEXT NOT NULL,
  statement TEXT NOT NULL,
  evidence TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active',
  run_id TEXT NOT NULL,
  UNIQUE(page_path, statement)
);

CREATE TABLE IF NOT EXISTS symbols (
  id TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  entity_id TEXT
);

CREATE TABLE IF NOT EXISTS page_links (
  from_path TEXT NOT NULL,
  to_path TEXT NOT NULL,
  PRIMARY KEY(from_path, to_path)
);

CREATE TABLE IF NOT EXISTS entities (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  canonical_key TEXT NOT NULL UNIQUE,
  props TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'active',
  updated_run TEXT
);

CREATE TABLE IF NOT EXISTS entity_aliases (
  entity_id TEXT NOT NULL,
  alias TEXT NOT NULL,
  PRIMARY KEY(entity_id, alias),
  FOREIGN KEY(entity_id) REFERENCES entities(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS relations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  src_id TEXT NOT NULL,
  dst_id TEXT NOT NULL,
  rel TEXT NOT NULL,
  props TEXT NOT NULL DEFAULT '{}',
  confidence REAL NOT NULL DEFAULT 1.0,
  run_id TEXT,
  UNIQUE(src_id, dst_id, rel),
  FOREIGN KEY(src_id) REFERENCES entities(id) ON DELETE CASCADE,
  FOREIGN KEY(dst_id) REFERENCES entities(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS chunks (
  id TEXT PRIMARY KEY,
  page_path TEXT NOT NULL,
  ord INTEGER NOT NULL,
  title TEXT NOT NULL,
  summary TEXT NOT NULL,
  body TEXT NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  source TEXT NOT NULL,
  body_hash TEXT NOT NULL,
  run_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chunk_entities (
  chunk_id TEXT NOT NULL,
  entity_key TEXT NOT NULL,
  PRIMARY KEY(chunk_id, entity_key),
  FOREIGN KEY(chunk_id) REFERENCES chunks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS embeddings (
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  vec BLOB NOT NULL,
  PRIMARY KEY(target_type, target_id)
);

CREATE INDEX IF NOT EXISTS idx_chunks_page ON chunks(page_path);
CREATE INDEX IF NOT EXISTS idx_entities_kind ON entities(kind, status);
CREATE INDEX IF NOT EXISTS idx_relations_src ON relations(src_id);
CREATE INDEX IF NOT EXISTS idx_relations_dst ON relations(dst_id);
CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);
"#;

/// Full-text index over chunk summary + body. The trigram tokenizer is used so
/// substring queries (code identifiers and CJK phrases) still match; terms it
/// cannot handle fall back to LIKE in `Store::search`.
pub const MIGRATION_FTS_TRIGRAM: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
  chunk_id UNINDEXED,
  page_path UNINDEXED,
  title,
  summary,
  body,
  tokenize = 'trigram'
);
"#;

/// Fallback for SQLite builds without the trigram tokenizer (< 3.34).
pub const MIGRATION_FTS_UNICODE61: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
  chunk_id UNINDEXED,
  page_path UNINDEXED,
  title,
  summary,
  body,
  tokenize = 'unicode61'
);
"#;
