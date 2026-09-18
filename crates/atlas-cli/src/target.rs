//! Resolve a project target and print run results for CLI commands.

use atlas_core::projects::ProjectRegistry;
use atlas_core::{pipeline, AtlasConfig};
use std::path::{Path, PathBuf};

pub(crate) struct ProjectTarget {
    pub(crate) id: String,
    pub(crate) repo_root: PathBuf,
    pub(crate) cfg: AtlasConfig,
}

pub(crate) fn resolve_target(launch_root: &Path, project: Option<&str>) -> anyhow::Result<ProjectTarget> {
    let raw = project.map(str::trim).unwrap_or("");
    let launch_cfg = AtlasConfig::load(launch_root)?;
    if raw.is_empty() || raw == "default" {
        return Ok(ProjectTarget {
            id: atlas_core::repo_slug(launch_root),
            repo_root: launch_root.to_path_buf(),
            cfg: launch_cfg,
        });
    }
    let reg = ProjectRegistry::load_with_discovery(launch_root);
    let pref = reg.resolve(Some(raw))?;
    Ok(ProjectTarget {
        id: pref.id,
        repo_root: pref.root,
        cfg: pref.cfg,
    })
}

pub(crate) fn apply_overrides(cfg: &mut AtlasConfig, provider: Option<String>, model: Option<String>) {
    if let Some(p) = provider {
        cfg.llm.provider = p;
    }
    if let Some(m) = model {
        cfg.llm.model = m;
    }
}

pub(crate) fn print_result(res: &pipeline::RunResult, project: &str, ci: bool) {
    let mut v = serde_json::to_value(res).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("project".into(), project.into());
    }
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    if !ci {
        return;
    }
    // CI: fail the step on a failed run or on structural problems, while
    // tolerating advisory notes (e.g. template fallback without an LLM key).
    let hard_problem = res.notes.iter().any(|n| n.starts_with("problem:"));
    if res.status == "failed" || hard_problem {
        std::process::exit(1);
    }
}
