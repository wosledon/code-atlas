//! Local filesystem helpers for export / web-dist discovery.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Locate the SPA build so `atlas web` works without extra setup.
/// Prefer an explicit `--web-dist`, then the target repo, then paths next to the binary
/// (covers `cargo build -p atlas-cli` from a source checkout and a simple install layout).
pub(crate) fn resolve_web_dist(
    explicit: Option<&Path>,
    repo_root: &Path,
) -> Option<PathBuf> {
    if let Some(d) = explicit {
        return d.join("index.html").exists().then(|| d.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = vec![repo_root.join("web/dist")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.extend([
            dir.join("web/dist"),
            dir.join("../web/dist"),
            dir.join("../../web/dist"),
            dir.join("../../../web/dist"),
        ]);
    }
    candidates
        .into_iter()
        .find(|d| d.join("index.html").exists())
}

pub(crate) fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        anyhow::bail!("wiki root missing: {}", src.display());
    }
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
