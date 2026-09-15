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

/// 一个文件里的一个声明（种类、名字、起始行号），由 [`crate::file_outline`] 产出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlineItem {
    pub line: usize,
    pub kind: String,
    pub name: String,
}
