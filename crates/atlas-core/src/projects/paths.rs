//! Registry / exe-relative control-file path resolution.

use super::types::REGISTRY_FILE;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Directory containing the running `atlas` (or test) executable.
pub fn atlas_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Resolve the project registry path.
///
/// Priority:
/// 1. `ATLAS_PROJECTS_FILE` env
/// 2. **`<atlas-exe-dir>/atlas.projects.json`** (control files follow the binary)
/// 3. `<launch-root>/atlas.projects.json` if the executable dir is unknown
pub fn registry_path_for(launch_root: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("ATLAS_PROJECTS_FILE") {
        let p = p.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Some(dir) = atlas_exe_dir() {
        return dir.join(REGISTRY_FILE);
    }
    launch_root.join(REGISTRY_FILE)
}

/// One-time copy of a repo-local registry into the exe-adjacent location.
pub(crate) fn migrate_legacy_registry(launch_root: &Path, target: &Path) {
    if target.exists() || target == launch_root.join(REGISTRY_FILE) {
        return;
    }
    let legacy = launch_root.join(REGISTRY_FILE);
    if !legacy.is_file() {
        return;
    }
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::copy(&legacy, target) {
        Ok(_) => tracing::info!(
            "migrated project registry {} -> {}",
            legacy.display(),
            target.display()
        ),
        Err(e) => tracing::warn!(
            "failed to migrate project registry {} -> {}: {e}",
            legacy.display(),
            target.display()
        ),
    }
}
