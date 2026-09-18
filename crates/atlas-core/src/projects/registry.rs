//! `ProjectRegistry`: launch project + registered/discovered repos.

use super::io::{read_entries, write_entries};
use super::markers::scan_project_markers;
use super::paths::{atlas_exe_dir, file_mtime, migrate_legacy_registry, registry_path_for};
use super::types::{
    ProjectEntry, ProjectRef, DEFAULT_PROJECT_ID, DISCOVER_TTL, EXE_DATA_DIR_NAME,
};
use crate::paths::repo_slug;
use crate::AtlasConfig;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Launch project + extras from the registry file.
///
/// The file on disk is the source of truth; `refresh_if_changed` reloads it
/// only when mtime changes (hot reload without re-reading every request).
#[derive(Debug, Clone)]
pub struct ProjectRegistry {
    pub(crate) launch_root: PathBuf,
    pub(crate) launch_id: String,
    pub(crate) registry_path: PathBuf,
    pub(crate) entries: Vec<ProjectEntry>,
    pub(crate) registry_mtime: Option<std::time::SystemTime>,
    pub(crate) last_discover: Option<Instant>,
}

impl ProjectRegistry {
    /// Load using the default registry path (next to the atlas executable).
    pub fn load(launch_root: &Path) -> Self {
        let registry_path = registry_path_for(launch_root);
        migrate_legacy_registry(launch_root, &registry_path);
        Self::load_with_registry_path(launch_root, registry_path)
    }

    /// Load with an explicit registry file location (tests / overrides).
    pub fn load_with_registry_path(launch_root: &Path, registry_path: PathBuf) -> Self {
        let entries = read_entries(&registry_path);
        let registry_mtime = file_mtime(&registry_path);
        Self {
            launch_root: launch_root.to_path_buf(),
            launch_id: repo_slug(launch_root),
            registry_path,
            entries,
            registry_mtime,
            last_discover: None,
        }
    }

    /// Load registry file + auto-discover centralized project markers.
    pub fn load_with_discovery(launch_root: &Path) -> Self {
        let mut reg = Self::load(launch_root);
        reg.discover_projects();
        reg.last_discover = Some(Instant::now());
        reg
    }

    /// Reload `entries` when the registry file mtime changed. Returns true if reloaded.
    pub fn refresh_if_changed(&mut self) -> bool {
        let mtime = file_mtime(&self.registry_path);
        if mtime == self.registry_mtime {
            return false;
        }
        self.entries = read_entries(&self.registry_path);
        self.registry_mtime = mtime;
        true
    }

    /// Discover markers, at most once per `DISCOVER_TTL`.
    pub fn discover_throttled(&mut self) -> Vec<ProjectEntry> {
        if let Some(t) = self.last_discover
            && t.elapsed() < DISCOVER_TTL
        {
            return Vec::new();
        }
        self.last_discover = Some(Instant::now());
        self.discover_projects()
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
                if self.entries.iter().any(|e| e.root == marker.root) {
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
        if !found.is_empty()
            && let Err(e) = self.save()
        {
            tracing::warn!("failed to persist discovered projects: {e:#}");
        }
        found
    }

    fn save(&mut self) -> Result<()> {
        write_entries(&self.registry_path, &self.entries)?;
        self.registry_mtime = file_mtime(&self.registry_path);
        self.last_discover = Some(Instant::now());
        Ok(())
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
            if let Some(b) = base
                && !bases.contains(&b)
            {
                bases.push(b);
            }
        }
        if let Some(dir) = atlas_exe_dir() {
            let b = dir.join(EXE_DATA_DIR_NAME);
            if !bases.contains(&b) {
                bases.push(b);
            }
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
            if root.as_os_str().is_empty() || root == self.launch_root {
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
    pub fn register(
        &mut self,
        root: &Path,
        id: Option<&str>,
        name: Option<&str>,
    ) -> Result<ProjectRef> {
        let root_buf = root.to_path_buf();
        if !root_buf.is_dir() {
            bail!("project root is not a directory: {}", root_buf.display());
        }
        if root_buf == self.launch_root {
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
        let root_str = root_buf.to_string_lossy().to_string();
        let entry = ProjectEntry {
            id: entry_id.clone(),
            name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            root: root_str.clone(),
        };
        if let Some(pos) = self.entries.iter().position(|e| e.id == entry_id) {
            self.entries[pos] = entry;
        } else if let Some(pos) = self.entries.iter().position(|e| e.root == root_str) {
            self.entries[pos] = entry;
        } else {
            self.entries.push(entry);
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
}
