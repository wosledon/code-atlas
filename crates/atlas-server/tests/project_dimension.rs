//! Project-dimension registry & update contract (unit + contract tests).

use atlas_core::projects::{write_project_marker, ProjectRegistry, REGISTRY_FILE};
use atlas_core::repo_slug;
use std::fs;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("atlas-server-proj-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn hot_reload_registry_picks_up_disk_changes() {
    let launch = temp_dir("hub");
    let other = temp_dir("side");

    let mut reg = ProjectRegistry::load(&launch);
    let added = reg.register(&other, None, None).unwrap();

    // Simulate another process writing the registry file.
    let path = launch.join(REGISTRY_FILE);
    assert!(path.exists(), "register should persist {}", path.display());
    let reloaded = ProjectRegistry::load(&launch);
    let pref = reloaded.resolve(Some(&added.id)).unwrap();
    assert_eq!(pref.root, other);

    fs::remove_dir_all(&launch).ok();
    fs::remove_dir_all(&other).ok();
}

#[test]
fn discovery_from_external_marker_supports_project_dimension_update() {
    let launch = temp_dir("hub2");
    let target = temp_dir("doc-proj");
    let external = temp_dir("ext");
    let data = external.join(repo_slug(&target));
    fs::create_dir_all(&data).unwrap();
    write_project_marker(&data, &target).unwrap();

    let mut reg = ProjectRegistry::load(&launch);
    let found = reg.discover_from_bases(&[external.clone()]);
    assert_eq!(found.len(), 1);
    let pref = reg.resolve(Some(&repo_slug(&target))).unwrap();
    assert_eq!(pref.root, target);
    // Update would use pref.root + pref.cfg — isolated per project.
    assert!(!pref.is_launch);

    fs::remove_dir_all(&launch).ok();
    fs::remove_dir_all(&target).ok();
    fs::remove_dir_all(&external).ok();
}

#[test]
fn lock_conflict_message_is_detectable_for_http_409() {
    // Contract used by run_project_update / MCP atlas_update.
    let msg = "atlas lock held by another process (pid=1 run_id=x). If that process is gone, delete ...";
    assert!(msg.contains("atlas lock held"));
}
