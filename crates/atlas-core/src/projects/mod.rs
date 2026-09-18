//! Project registry: which repositories this Atlas surface can list / update.
//!
//! The launch repository is always present (id = directory name, alias
//! `default`). Extra projects are declared in **`atlas.projects.json` next to
//! the `atlas` executable** (or `ATLAS_PROJECTS_FILE`). Storage under each
//! project stays isolated: update resolves a project to its own `repo_root` +
//! `AtlasConfig` + atlas/wiki path.
//!
//! Modules: [`types`] (DTOs), [`paths`] (exe-adjacent control files),
//! [`markers`] (discovery identity files), [`io`] (registry file),
//! [`registry`] (`ProjectRegistry`).

mod io;
mod markers;
mod paths;
mod registry;
mod types;

#[cfg(test)]
mod tests;

pub use markers::{read_project_marker, write_project_marker};
pub use paths::{atlas_exe_dir, registry_path_for};
pub use registry::ProjectRegistry;
pub use types::{
    ProjectEntry, ProjectMarker, ProjectRef, DEFAULT_PROJECT_ID, EXE_DATA_DIR_NAME,
    PROJECT_MARKER_FILE, REGISTRY_FILE,
};
