//! `atlas project` subcommands (registry management).

use atlas_core::projects::ProjectRegistry;
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub(crate) enum ProjectCmd {
    /// List launch project + registered / discovered projects
    List,
    /// Register a repository root in atlas.projects.json (next to the atlas executable)
    Add {
        root: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a registered project (launch project cannot be removed)
    Remove { id: String },
    /// Scan external_root markers and merge new projects into the registry
    Discover,
}

pub(crate) fn run_project_cmd(launch_root: &Path, cmd: ProjectCmd) -> anyhow::Result<()> {
    match cmd {
        ProjectCmd::List => {
            let mut reg = ProjectRegistry::load_with_discovery(launch_root);
            let discovered = reg.discover_projects();
            if !discovered.is_empty() {
                eprintln!(
                    "discovered {} project(s) from external_root",
                    discovered.len()
                );
            }
            let list = reg.list();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "launch": reg.launch_id(),
                    "registry_path": reg.registry_path().display().to_string(),
                    "projects": list.iter().map(|p| serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "root": p.root.display().to_string(),
                        "atlas_root": p.atlas_root.display().to_string(),
                        "is_launch": p.is_launch,
                    })).collect::<Vec<_>>(),
                }))?
            );
        }
        ProjectCmd::Add { root, id, name } => {
            let mut reg = ProjectRegistry::load_with_discovery(launch_root);
            let pref = reg.register(&root, id.as_deref(), name.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": pref.id,
                    "name": pref.name,
                    "root": pref.root.display().to_string(),
                    "registry_path": reg.registry_path().display().to_string(),
                }))?
            );
        }
        ProjectCmd::Remove { id } => {
            let mut reg = ProjectRegistry::load_with_discovery(launch_root);
            reg.unregister(&id)?;
            println!("removed project `{id}`");
        }
        ProjectCmd::Discover => {
            let mut reg = ProjectRegistry::load_with_discovery(launch_root);
            let found = reg.discover_projects();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "discovered": found.iter().map(|e| serde_json::json!({
                        "id": e.id,
                        "root": e.root,
                    })).collect::<Vec<_>>(),
                    "projects": reg.list().iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
                }))?
            );
        }
    }
    Ok(())
}
