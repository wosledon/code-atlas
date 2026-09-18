//! Project identity markers under atlas data dirs (discovery).

use super::types::{ProjectMarker, PROJECT_MARKER_FILE};
use crate::paths::repo_slug;
use anyhow::Result;
use std::path::Path;

/// Persist project identity next to atlas data so a hub can discover it later.
pub fn write_project_marker(data_dir: &Path, repo_root: &Path) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let marker = ProjectMarker {
        id: repo_slug(repo_root),
        name: Some(repo_slug(repo_root)),
        root: portable_path(repo_root),
    };
    let path = data_dir.join(PROJECT_MARKER_FILE);
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&marker)?))?;
    Ok(())
}

/// Absolute path without Windows `\\?\` prefix (serde-friendly).
fn portable_path(p: &Path) -> String {
    let abs = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    s.strip_prefix(r"\\?\")
        .map(str::to_string)
        .unwrap_or(s)
        .replace('\\', "/")
}

pub fn read_project_marker(data_dir: &Path) -> Option<ProjectMarker> {
    let text = std::fs::read_to_string(data_dir.join(PROJECT_MARKER_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn scan_project_markers(base: &Path) -> Vec<ProjectMarker> {
    let mut out = Vec::new();
    if !base.is_dir() {
        return out;
    }
    let Ok(rd) = std::fs::read_dir(base) else {
        return out;
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if let Some(m) = read_project_marker(&dir) {
            out.push(m);
            continue;
        }
        // Nested layout: <base>/<slug>/data/.atlas-project.json
        if let Some(m) = read_project_marker(&dir.join("data")) {
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn portable_path_str(p: &Path) -> String {
    portable_path(p)
}
