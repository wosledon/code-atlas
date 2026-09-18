use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::paths::repo_slug;

mod env;
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
        env::apply_env(&mut cfg);
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

    /// Files Atlas itself maintains inside the repository: the database (and
    /// its WAL sidecars and the run lock) plus the scaffold it writes —
    /// `AGENTS.md` pointer block and `atlas.toml.example`. Generation never
    /// reads them, but they change while a run is in progress, so both the
    /// repository scan and the page fingerprints must ignore them; otherwise
    /// every run looks like "the repository changed" and nothing is ever reused.
    pub fn ignored_scan_files(&self, repo_root: &Path) -> Vec<PathBuf> {
        let db = self.db_path(repo_root);
        let dir = db.parent().unwrap_or(repo_root).to_path_buf();
        let mut out = vec![
            db.clone(),
            dir.join("atlas.lock"),
            dir.join("atlas.lock.heartbeat"),
            repo_root.join("AGENTS.md"),
            repo_root.join("atlas.toml.example"),
            // Multi-project registry lives next to the launch repo; editing it
            // must not look like a repository change on every update.
            repo_root.join(crate::REGISTRY_FILE),
            // Project identity marker written next to atlas data.
            db.parent()
                .unwrap_or(repo_root)
                .to_path_buf()
                .join(crate::PROJECT_MARKER_FILE),
        ];
        for suffix in ["-wal", "-shm"] {
            let mut name = db.file_name().unwrap_or_default().to_os_string();
            name.push(suffix);
            out.push(dir.join(name));
        }
        out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_maintained_files_are_ignored_by_the_scan() {
        let cfg = AtlasConfig::default();
        let root = Path::new("/repo");
        let ignored = cfg.ignored_scan_files(root);
        for expected in [
            "data/atlas.db",
            "data/atlas.db-wal",
            "data/atlas.lock",
            "AGENTS.md",
            "atlas.toml.example",
            crate::REGISTRY_FILE,
            crate::PROJECT_MARKER_FILE,
        ] {
            let needle = Path::new("/repo").join(expected);
            // Marker sits next to the db (data/), not always at repo root.
            let alt = Path::new("/repo/data").join(crate::PROJECT_MARKER_FILE);
            assert!(
                ignored.contains(&needle) || ignored.contains(&alt),
                "{expected} missing from {ignored:?}"
            );
        }
    }
}
