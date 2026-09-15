use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoScan {
    pub root: PathBuf,
    pub files: Vec<SourceFile>,
    pub readme_excerpt: Option<String>,
    pub manifests: Vec<PathBuf>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub rel: String,
    pub language: Option<String>,
    pub size: u64,
}
