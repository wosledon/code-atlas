use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::ignore::{IgnoreRules, SKIP_DIRS};
use crate::types::{RepoScan, SourceFile};

pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Some("rust"),
        Some("ts") | Some("tsx") | Some("mts") | Some("cts") => Some("typescript"),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Some("javascript"),
        Some("py") | Some("pyi") => Some("python"),
        Some("go") => Some("go"),
        Some("java") => Some("java"),
        Some("kt") | Some("kts") => Some("kotlin"),
        Some("c") | Some("h") => Some("c"),
        Some("cpp") | Some("cc") | Some("hpp") | Some("cxx") => Some("cpp"),
        _ => None,
    }
}

pub fn scan_repo(root: &Path) -> anyhow::Result<RepoScan> {
    scan_repo_with_skips(root, &[])
}

/// Scan the repository, additionally ignoring `skips` — absolute paths of the
/// files Atlas itself writes (store database, WAL, run lock). Those change on
/// every run, so keeping them in the scan would change every page fingerprint
/// and silently disable incremental reuse.
pub fn scan_repo_with_skips(root: &Path, skips: &[PathBuf]) -> anyhow::Result<RepoScan> {
    let ignore = IgnoreRules::load_root(root);
    let mut files = Vec::new();
    let mut readme_excerpt = None;
    let mut manifests = Vec::new();
    let mut langs = std::collections::BTreeSet::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if skips.iter().any(|s| e.path().starts_with(s)) {
                return false;
            }
            let name = e.file_name().to_string_lossy();
            if SKIP_DIRS.contains(&name.as_ref()) {
                return false;
            }
            let rel = e
                .path()
                .strip_prefix(root)
                .unwrap_or(e.path())
                .to_string_lossy()
                .replace('\\', "/");
            !ignore.is_ignored(&rel, e.file_type().is_dir())
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if ignore.is_ignored(&rel, false) {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_ascii_lowercase();

        if lower == "readme.md" || lower == "readme" {
            if let Ok(text) = std::fs::read_to_string(path) {
                let excerpt: String = text.chars().take(4000).collect();
                readme_excerpt = Some(excerpt);
            }
            continue;
        }
        if matches!(
            lower.as_str(),
            "cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "pom.xml"
        ) {
            manifests.push(path.to_path_buf());
        }

        let language = detect_language(path);
        if let Some(lang) = language {
            langs.insert(lang.to_string());
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if language.is_some() || size < 2_000_000 {
            files.push(SourceFile {
                path: path.to_path_buf(),
                rel,
                language: language.map(|s| s.to_string()),
                size,
            });
        }
    }

    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    // Cap for MVP discovery noise
    if files.len() > 4000 {
        files.truncate(4000);
    }

    Ok(RepoScan {
        root: root.to_path_buf(),
        files,
        readme_excerpt,
        manifests,
        languages: langs.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_files_are_skipped() {
        let root = std::env::temp_dir().join(format!(
            "atlas-scan-skip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        let db = root.join("data/atlas.db");
        std::fs::write(&db, b"sqlite-ish bytes").unwrap();
        std::fs::write(root.join("data/atlas.lock"), b"123").unwrap();

        let plain = scan_repo(&root).expect("scan");
        assert!(plain.files.iter().any(|f| f.rel == "data/atlas.db"));

        let skips = vec![
            db.clone(),
            root.join("data/atlas.lock"),
            root.join("data/atlas.db-wal"),
        ];
        let filtered = scan_repo_with_skips(&root, &skips).expect("scan");
        assert!(filtered.files.iter().any(|f| f.rel == "src/lib.rs"));
        assert!(!filtered.files.iter().any(|f| f.rel == "data/atlas.db"));
        assert!(!filtered.files.iter().any(|f| f.rel == "data/atlas.lock"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
