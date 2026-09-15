//! Page planning, evidence gathering, LLM generation and maintenance tasks.
//!
//! `atlas init|update` runs through [`run_init_or_update`], which chains the
//! phase modules: `plan` decides the page set, `prepare` fingerprints it for
//! incremental reuse, `generate` renders bodies (LLM or template), `write`
//! persists pages/claims/chunks, `graph` seeds the entity graph and
//! `maintenance` finishes the run. Supporting modules: `evidence` (prompt
//! assembly), `plan_modules` (repository layout detection) and `template`
//! (offline fallback).
//! Everything the CLI and the server use is re-exported here so
//! `atlas_core::pipeline::*` stays a stable API.
use crate::git;
use crate::lock::RunLock;
use crate::markdown::{self, FrontMatter};
use crate::tools::RepoTools;
use crate::{ensure_agents_pointer, sha256_hex, AtlasConfig};
use anyhow::{bail, Result};
use atlas_analyze::{extract_symbols, read_file_excerpt, scan_repo, RepoScan, SourceFile};
use atlas_claims::{claims_from_markdown, write_page_claims};
use atlas_kb::{link_page_to_module, store_chunks, upsert_module_entities, ChunkMode};
use atlas_llm::{LlmClient, LlmConfig};
use atlas_store::{body_hash, Store};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

mod evidence;
mod generate;
mod graph;
mod maintenance;
mod plan;
mod plan_modules;
mod prepare;
mod run;
mod template;
mod types;
mod write;

#[cfg(test)]
mod tests;

pub use maintenance::{open_store, reindex, run_check};
pub use plan::{preview_plan, PlannedPage};
pub use run::run_init_or_update;
pub use types::{LastUpdate, PipelineCtx, RunResult};
