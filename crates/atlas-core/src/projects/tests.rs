use super::*;
use crate::paths::repo_slug;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("atlas-projects-{name}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    p
}

fn test_registry(launch: &Path) -> ProjectRegistry {
    ProjectRegistry::load_with_registry_path(launch, launch.join("test.registry.json"))
}

#[test]
fn launch_project_resolves_by_default_alias_and_slug() {
    let root = temp_dir("launch");
    let reg = test_registry(&root);
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
    let mut reg = test_registry(&launch);
    let added = reg.register(&other, None, Some("Side")).unwrap();
    assert_eq!(added.id, repo_slug(&other));
    assert_eq!(added.name, "Side");
    assert!(!added.is_launch);
    assert!(
        reg.registry_path().exists(),
        "registry file missing at {:?}",
        reg.registry_path()
    );

    let again = ProjectRegistry::load_with_registry_path(&launch, reg.registry_path().to_path_buf());
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
    let mut reg = test_registry(&root);
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

    let mut reg = test_registry(&launch);
    let discovered = reg.discover_from_bases(std::slice::from_ref(&external));
    let again = reg.discover_from_bases(std::slice::from_ref(&external));
    let portable = super::markers::portable_path_str(&other);
    assert!(
        discovered
            .iter()
            .any(|e| e.root == other.to_string_lossy() || e.root == portable || e.id == repo_slug(&other)),
        "discovered={discovered:?}, other={other:?}"
    );
    assert!(again.is_empty());
    assert!(reg.resolve(Some(&repo_slug(&other))).is_ok());

    fs::remove_dir_all(&launch).ok();
    fs::remove_dir_all(&other).ok();
    fs::remove_dir_all(&external).ok();
}

#[test]
fn registry_path_prefers_exe_dir() {
    let launch = temp_dir("path-launch");
    let path = registry_path_for(&launch);
    if let Some(dir) = atlas_exe_dir() {
        assert_eq!(path, dir.join(REGISTRY_FILE));
    } else {
        assert_eq!(path, launch.join(REGISTRY_FILE));
    }
    fs::remove_dir_all(&launch).ok();
}

#[test]
fn refresh_if_changed_skips_same_mtime_and_picks_up_writes() {
    let launch = temp_dir("mtime");
    let other = temp_dir("mtime-side");
    let mut reg = test_registry(&launch);
    assert!(!reg.refresh_if_changed(), "first refresh should be a no-op");

    let added = reg.register(&other, None, None).unwrap();
    assert!(!reg.refresh_if_changed());

    let path = reg.registry_path().to_path_buf();
    let mut text = fs::read_to_string(&path).unwrap();
    text = text.replace("mtime-side", "mtime-side2");
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&path, &text).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));

    assert!(reg.refresh_if_changed(), "external write should reload");
    let _ = added;
    fs::remove_dir_all(&launch).ok();
    fs::remove_dir_all(&other).ok();
}
