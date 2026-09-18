//! Registry value types and path constants.

use crate::AtlasConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

pub const REGISTRY_FILE: &str = "atlas.projects.json";
pub const DEFAULT_PROJECT_ID: &str = "default";
/// Written into a project's atlas data dir so centralized hubs can discover it.
pub const PROJECT_MARKER_FILE: &str = ".atlas-project.json";
/// Default centralized data root next to the atlas executable.
pub const EXE_DATA_DIR_NAME: &str = ".atlas-data";
/// How long resolve/list may skip a re-scan of external markers.
pub(crate) const DISCOVER_TTL: Duration = Duration::from_secs(15);

/// One registered (non-launch) project on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub root: String,
}

/// Marker written next to `atlas.db` / wiki for external-dir discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMarker {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Absolute repository root (the scan/update target).
    pub root: String,
}

/// Resolved project context used by update / read APIs.
#[derive(Debug, Clone)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub cfg: AtlasConfig,
    pub atlas_root: PathBuf,
    /// True for the repository this process was started against.
    pub is_launch: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RegistryFile {
    #[serde(default)]
    pub(crate) projects: Vec<ProjectEntry>,
}
