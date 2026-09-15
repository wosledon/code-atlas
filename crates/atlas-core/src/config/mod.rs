use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::paths::repo_slug;

mod sections;

pub use sections::{
    AnalyzeSection, ChunkSection, GraphSection, KbSection, LlmSection, OutputSection,
    PrivacySection,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasConfig {
    #[serde(default)]
    pub llm: LlmSection,
    #[serde(default)]
    pub privacy: PrivacySection,
    #[serde(default)]
    pub graph: GraphSection,
    #[serde(default)]
    pub kb: KbSection,
    #[serde(default)]
    pub output: OutputSection,
    #[serde(default)]
    pub analyze: AnalyzeSection,
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            llm: LlmSection::default(),
            privacy: PrivacySection::default(),
            graph: GraphSection::default(),
            kb: KbSection::default(),
            output: OutputSection::default(),
            analyze: AnalyzeSection::default(),
        }
    }
}

impl AtlasConfig {
    pub fn load(repo_root: &Path) -> Result<Self> {
        let mut cfg = AtlasConfig::default();
        for name in ["atlas.toml", "atlas.config.json"] {
            let p = repo_root.join(name);
            if !p.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&p)?;
            if name.ends_with(".json") {
                cfg = serde_json::from_str(&text)?;
            } else {
                cfg = toml::from_str(&text)?;
            }
            break;
        }
        // env overrides
        if let Ok(p) = std::env::var("ATLAS_PROVIDER") {
            cfg.llm.provider = p;
        }
        if let Ok(m) = std::env::var("ATLAS_MODEL") {
            cfg.llm.model = m;
        }
        if let Ok(u) = std::env::var("ATLAS_BASE_URL") {
            cfg.llm.base_url = u;
        }
        if let Ok(t) = std::env::var("ATLAS_TEMPERATURE") {
            if let Ok(v) = t.parse() {
                cfg.llm.temperature = v;
            }
        }
        if let Ok(t) = std::env::var("ATLAS_MAX_OUTPUT_TOKENS") {
            if let Ok(v) = t.parse() {
                cfg.llm.max_output_tokens = v;
            }
        }
        if let Ok(t) = std::env::var("ATLAS_TIMEOUT_SECS") {
            if let Ok(v) = t.parse() {
                cfg.llm.timeout_secs = v;
            }
        }
        if let Ok(t) = std::env::var("ATLAS_CONCURRENCY") {
            if let Ok(v) = t.parse() {
                cfg.llm.concurrency = v;
            }
        }
        if let Ok(t) = std::env::var("ATLAS_TOOL_ROUNDS") {
            if let Ok(v) = t.parse() {
                cfg.llm.max_tool_rounds = v;
            }
        }
        if let Ok(lang) = std::env::var("ATLAS_OUTPUT_LANGUAGE") {
            cfg.output.language = lang;
        }
        Ok(cfg)
    }

    pub fn db_path(&self, repo_root: &Path) -> PathBuf {
        match self.output.strategy.as_str() {
            "external-dir" | "db-only" => {
                let base = if self.output.external_root.is_empty() {
                    repo_root.join(".atlas-data")
                } else {
                    PathBuf::from(&self.output.external_root)
                };
                let slug = repo_slug(repo_root);
                base.join(slug).join("atlas.db")
            }
            _ => repo_root.join("data").join("atlas.db"),
        }
    }

    pub fn atlas_root(&self, repo_root: &Path) -> PathBuf {
        match self.output.strategy.as_str() {
            "external-dir" => {
                let base = if self.output.external_root.is_empty() {
                    repo_root.join(".atlas-data")
                } else {
                    PathBuf::from(&self.output.external_root)
                };
                base.join(repo_slug(repo_root)).join("wiki")
            }
            "db-only" => repo_root.join(".atlas-data").join(repo_slug(repo_root)).join("wiki-export"),
            _ => repo_root.join(&self.output.atlas_root),
        }
    }
}
