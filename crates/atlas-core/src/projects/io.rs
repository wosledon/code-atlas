//! Registry file load/save.

use super::paths::file_mtime;
use super::types::{ProjectEntry, RegistryFile};
use std::path::Path;

pub(crate) fn read_entries(path: &Path) -> Vec<ProjectEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<RegistryFile>(&text) {
        Ok(f) => f.projects,
        Err(e) => {
            tracing::warn!("invalid {}: {e}", path.display());
            Vec::new()
        }
    }
}

pub(crate) fn write_entries(path: &Path, entries: &[ProjectEntry]) -> anyhow::Result<()> {
    let file = RegistryFile {
        projects: entries.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{text}\n"))?;
    let _ = file_mtime(path);
    Ok(())
}
