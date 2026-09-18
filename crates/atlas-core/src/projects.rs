//! Project registry: which repositories this Atlas surface can list / update.
//!
//! The launch repository is always present (id = directory name, alias
//! `default`). Extra projects are declared in `atlas.projects.json` next to the
//! launch root, or in the file named by `ATLAS_PROJECTS_FILE`. Storage under
//! each project stays isolated: update resolves a project to its own
//! `repo_root` + `AtlasConfig` + atlas/wiki path.

use crate::paths::repo_slug;
use crate::AtlasConfig;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const REGISTRY_FILE: &str = "atlas.projects.json";
pub const DEFAULT_PROJECT_ID: &str = "default";
/// Written into a project's atlas data dir so centralized hubs can discover it.
pub const PROJECT_MARKER_FILE: &str = ".atlas-project.json";

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
struct RegistryFile {
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

/// Launch project + extras from the registry file.
#[derive(Debug, Clone)]
pub struct ProjectRegistry {
    launch_root: PathBuf,
    launch_id: String,
    registry_path: PathBuf,
    entries: Vec<ProjectEntry>,
}

impl ProjectRegistry {
    pub fn load(launch_root: &Path) -> Self {
        let registry_path = registry_path_for(launch_root);
        let entries = read_entries(&registry_path);
        Self {
            launch_root: launch_root.to_path_buf(),
            launch_id: repo_slug(launch_root),
            registry_path,
            entries,
        }
    }

    /// Load registry file + auto-discover centralized project markers.
    pub fn load_with_discovery(launch_root: &Path) -> Self {
        let mut reg = Self::load(launch_root);
        reg.discover_projects();
        reg
    }

    /// Scan `external_root` (and env override) for `.atlas-project.json` markers
    /// and merge missing projects into the in-memory registry (persisted).
    pub fn discover_projects(&mut self) -> Vec<ProjectEntry> {
        let bases = self.external_roots();
        self.discover_from_bases(&bases)
    }

    /// Merge projects found under the given data-root directories.
    pub fn discover_from_bases(&mut self, bases: &[PathBuf]) -> Vec<ProjectEntry> {
        let mut found = Vec::new();
        for base in bases {
            for marker in scan_project_markers(base) {
                let root = PathBuf::from(&marker.root);
                if root.as_os_str().is_empty() || !root.is_dir() || root == self.launch_root {
                    continue;
                }
                if self.launch_id == marker.id || self.entries.iter().any(|e| e.id == marker.id) {
                    continue;
                }
                if self.entries.iter().any(|e| PathBuf::from(&e.root) == root) {
                    continue;
                }
                let entry = ProjectEntry {
                    id: marker.id.clone(),
                    name: marker.name.filter(|s| !s.trim().is_empty()),
                    root: marker.root.clone(),
                };
                self.entries.push(entry.clone());
                found.push(entry);
            }
        }
        if !found.is_empty() && let Err(e) = self.save() {
            tracing::warn!("failed to persist discovered projects: {e:#}");
        }
        found
    }

    fn external_roots(&self) -> Vec<PathBuf> {
        let mut bases = Vec::new();
        if let Ok(v) = std::env::var("ATLAS_EXTERNAL_ROOT") {
            let v = v.trim();
            if !v.is_empty() {
                bases.push(PathBuf::from(v));
            }
        }
        if let Ok(cfg) = AtlasConfig::load(&self.launch_root) {
            let base = if cfg.output.external_root.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(cfg.output.external_root.trim()))
            };
            if let Some(b) = base {
                if !bases.contains(&b) {
                    bases.push(b);
                }
            }
            // Also scan sibling data dirs when strategy uses per-repo .atlas-data
            // is intentionally skipped: without a marker we cannot recover root.
            let _ = cfg.output.strategy;
        }
        bases
    }

    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    /// Launch project first, then registered roots that still exist.
    pub fn list(&self) -> Vec<ProjectRef> {
        let mut out = vec![self.launch_ref()];
        for e in &self.entries {
            let root = PathBuf::from(&e.root);
            if root.as_os_str().is_empty() {
                continue;
            }
            if root == self.launch_root {
                continue;
            }
            match Self::ref_from_entry(e) {
                Ok(r) => out.push(r),
                Err(err) => {
                    tracing::warn!("skip project `{}`: {err:#}", e.id);
                }
            }
        }
        out
    }

    /// Resolve by id. `None` / empty / `default` / launch slug → launch project.
    pub fn resolve(&self, id: Option<&str>) -> Result<ProjectRef> {
        let raw = id.map(str::trim).unwrap_or("");
        if raw.is_empty() || raw == DEFAULT_PROJECT_ID || raw == self.launch_id {
            return Ok(self.launch_ref());
        }
        let entry = self
            .entries
            .iter()
            .find(|e| e.id == raw)
            .ok_or_else(|| anyhow::anyhow!("unknown project id: `{raw}`"))?;
        Self::ref_from_entry(entry)
    }

    /// Register (or refresh) a project by root path. Id defaults to directory name.
    pub fn register(&mut self, root: &Path, id: Option<&str>, name: Option<&str>) -> Result<ProjectRef> {
        let root_buf = root.to_path_buf();
        if !root_buf.is_dir() {
            bail!("project root is not a directory: {}", root_buf.display());
        }
        if root_buf == self.launch_root {
            // Launch project is implicit; do not write it into the registry file.
            let mut r = self.launch_ref();
            if let Some(n) = name.filter(|s| !s.trim().is_empty()) {
                r.name = n.trim().to_string();
            }
            return Ok(r);
        }
        let entry_id = match id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => v.to_string(),
            None => repo_slug(&root_buf),
        };
        if entry_id == DEFAULT_PROJECT_ID {
            bail!("`{DEFAULT_PROJECT_ID}` is reserved for the launch project");
        }
        if entry_id == self.launch_id {
            bail!("project id `{entry_id}` collides with the launch project");
        }
        if let Some(pos) = self.entries.iter().position(|e| e.id == entry_id) {
            self.entries[pos] = ProjectEntry {
                id: entry_id.clone(),
                name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                root: root_buf.to_string_lossy().to_string(),
            };
        } else if let Some(pos) = self.entries.iter().position(|e| PathBuf::from(&e.root) == root_buf)
        {
            self.entries[pos] = ProjectEntry {
                id: entry_id.clone(),
                name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                root: root_buf.to_string_lossy().to_string(),
            };
        } else {
            self.entries.push(ProjectEntry {
                id: entry_id.clone(),
                name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                root: root_buf.to_string_lossy().to_string(),
            });
        }
        self.save()?;
        self.resolve(Some(&entry_id))
    }

    /// Remove a registered project. Launch project cannot be removed.
    pub fn unregister(&mut self, id: &str) -> Result<()> {
        let raw = id.trim();
        if raw.is_empty() || raw == DEFAULT_PROJECT_ID || raw == self.launch_id {
            bail!("cannot unregister the launch project (`{raw}`)");
        }
        let before = self.entries.len();
        self.entries.retain(|e| e.id != raw);
        if self.entries.len() == before {
            bail!("unknown project id: `{raw}`");
        }
        self.save()
    }

    fn launch_ref(&self) -> ProjectRef {
        let cfg = AtlasConfig::load(&self.launch_root).unwrap_or_default();
        let atlas_root = cfg.atlas_root(&self.launch_root);
        ProjectRef {
            id: self.launch_id.clone(),
            name: self.launch_id.clone(),
            root: self.launch_root.clone(),
            cfg,
            atlas_root,
            is_launch: true,
        }
    }

    fn ref_from_entry(e: &ProjectEntry) -> Result<ProjectRef> {
        let root = PathBuf::from(&e.root);
        if !root.is_dir() {
            bail!("root not found: {}", root.display());
        }
        let cfg = AtlasConfig::load(&root)?;
        let atlas_root = cfg.atlas_root(&root);
        let name = e
            .name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| repo_slug(&root));
        Ok(ProjectRef {
            id: e.id.clone(),
            name,
            root,
            cfg,
            atlas_root,
            is_launch: false,
        })
    }

    fn save(&self) -> Result<()> {
        let file = RegistryFile {
            projects: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&file)?;
        if let Some(parent) = self.registry_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.registry_path, format!("{text}\n"))?;
        Ok(())
    }
}

pub fn registry_path_for(launch_root: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("ATLAS_PROJECTS_FILE") {
        let p = p.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    launch_root.join(REGISTRY_FILE)
}

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

fn scan_project_markers(base: &Path) -> Vec<ProjectMarker> {
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

fn read_entries(path: &Path) -> Vec<ProjectEntry> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("atlas-projects-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn launch_project_resolves_by_default_alias_and_slug() {
        let root = temp_dir("launch");
        let reg = ProjectRegistry::load(&root);
        let launch = reg.resolve(None).unwrap();
        assert_eq!(launch.id, repo_slug(&root));
        assert!(launch.is_launch);
        assert!(reg.resolve(Some(DEFAULT_PROJECT_ID)).unwrap().is_launch);
        assert!(reg.resolve(Some(&launch.id)).unwrap().is_launch);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn register_resolve_unregister_roundtrip() {
        let launch = temp_dir("hub");
        let other = temp_dir("side-proj");
        let mut reg = ProjectRegistry::load(&launch);
        let added = reg.register(&other, None, Some("Side")).unwrap();
        assert_eq!(added.id, repo_slug(&other));
        assert_eq!(added.name, "Side");
        assert!(!added.is_launch);
        assert!(reg.registry_path().exists(), "registry file missing at {:?}", reg.registry_path());

        let again = ProjectRegistry::load(&launch);
        let resolved = again.resolve(Some(&added.id)).unwrap();
        assert_eq!(resolved.root, other);
        assert!(again.list().iter().any(|p| p.id == added.id));

        let mut again = again;
        again.unregister(&added.id).unwrap();
        assert!(again.resolve(Some(&added.id)).is_err());
        fs::remove_dir_all(&launch).ok();
        fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn cannot_unregister_launch() {
        let root = temp_dir("keep");
        let mut reg = ProjectRegistry::load(&root);
        let launch_id = reg.launch_id().to_string();
        assert!(reg.unregister(DEFAULT_PROJECT_ID).is_err());
        assert!(reg.unregister(&launch_id).is_err());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discovers_projects_from_external_markers() {
        let launch = temp_dir("hub2");
        let other = temp_dir("central-proj");
        let external = temp_dir("atlas-external");
        let data_dir = external.join(repo_slug(&other));
        fs::create_dir_all(&data_dir).unwrap();
        write_project_marker(&data_dir, &other).unwrap();

        let mut reg = ProjectRegistry::load(&launch);
        let discovered = reg.discover_from_bases(&[external.clone()]);
        let again = reg.discover_from_bases(&[external.clone()]);
        assert!(
            discovered.iter().any(|e| {
                PathBuf::from(&e.root) == other
                    || PathBuf::from(&e.root) == portable_path(&other)
                    || e.id == repo_slug(&other)
            }),
            "discovered={discovered:?}, other={other:?}"
        );
        assert!(again.is_empty());
        assert!(reg.resolve(Some(&repo_slug(&other))).is_ok());

        fs::remove_dir_all(&launch).ok();
        fs::remove_dir_all(&other).ok();
        fs::remove_dir_all(&external).ok();
    }
}
