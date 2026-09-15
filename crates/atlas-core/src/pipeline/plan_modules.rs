use super::*;

/// 通用：从任意仓库布局发现「代码模块」（不绑定 crates / atlas-*）。
/// 返回 (stable_key, display_label, path_prefix_hint)
pub(crate) fn detect_code_modules(scan: &RepoScan) -> Vec<(String, String, String)> {
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    const SKIP: &[&str] = &[
        "node_modules", "target", "dist", "build", "vendor", "out", "coverage",
        ".git", ".github", ".idea", ".vscode", ".venv", "venv", "__pycache__",
        "docs", "doc", "atlas", "test", "tests", "testdata", "fixtures",
    ];

    let is_pkg_file = |f: &SourceFile| -> bool {
        let r = f.rel.as_str();
        f.language.is_some()
            || r.ends_with("Cargo.toml")
            || r.ends_with("package.json")
            || r.ends_with("pyproject.toml")
            || r.ends_with("go.mod")
            || r.ends_with("pom.xml")
            || r.ends_with("build.gradle")
            || r.ends_with("CMakeLists.txt")
    };

    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1) monorepo package roots（crates/packages/apps/services/libs/modules/...）
    let mono_roots = [
        "crates", "packages", "apps", "services", "libs", "modules", "components",
        "pkg", "cmd", "internal",
    ];
    for root in mono_roots {
        let prefix = format!("{root}/");
        let mut groups: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
        for f in scan.files.iter().filter(|f| is_pkg_file(f)) {
            if let Some(rest) = f.rel.strip_prefix(prefix.as_str()) {
                let name = rest.split('/').next().unwrap_or("");
                if name.is_empty() || SKIP.contains(&name) {
                    continue;
                }
                let e = groups.entry(name.to_string()).or_default();
                e.0 += 1;
                if let Some(lang) = &f.language {
                    e.1.insert(lang.clone());
                }
            }
        }
        for (name, (count, langs)) in groups {
            if count < 1 {
                continue;
            }
            let key = format!("{root}/{name}");
            if !seen.insert(key.clone()) {
                continue;
            }
            let lang_note = if langs.is_empty() {
                String::new()
            } else {
                format!(" [{}]", langs.iter().take(3).cloned().collect::<Vec<_>>().join("/"))
            };
            out.push((
                key.clone(),
                format!("{name}{lang_note}"),
                format!("{root}/{name}/"),
            ));
        }
        if !out.is_empty() {
            break; // 已识别 monorepo 布局
        }
    }

    // 2) src/* 子目录（单体仓库）
    if out.is_empty() {
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for f in scan.files.iter().filter(|f| f.language.is_some()) {
            if let Some(rest) = f.rel.strip_prefix("src/") {
                let name = rest.split('/').next().unwrap_or("");
                if name.is_empty() || !rest.contains('/') {
                    // src/lib.rs 等根文件 → 记为 src
                    *groups.entry("src".into()).or_default() += 1;
                    continue;
                }
                if SKIP.contains(&name) {
                    continue;
                }
                *groups.entry(name.to_string()).or_default() += 1;
            }
        }
        // 若只有 src 根文件，也保留一个 src 模块
        for (name, count) in groups {
            if count < 1 {
                continue;
            }
            let key = format!("src/{name}");
            if !seen.insert(key.clone()) {
                continue;
            }
            let hint = if name == "src" {
                "src/".into()
            } else {
                format!("src/{name}/")
            };
            out.push((key, name, hint));
        }
    }

    // 3) 顶层有代码的目录（最终回退）
    if out.is_empty() {
        let mut tops: BTreeMap<String, usize> = BTreeMap::new();
        for f in scan.files.iter().filter(|f| f.language.is_some()) {
            let top = f.rel.split('/').next().unwrap_or("");
            if top.is_empty() || SKIP.contains(&top) {
                continue;
            }
            *tops.entry(top.to_string()).or_default() += 1;
        }
        for (name, count) in tops {
            if count < 1 {
                continue;
            }
            if !seen.insert(name.clone()) {
                continue;
            }
            let hint = format!("{name}/");
            out.push((name.clone(), name, hint));
        }
    }

    // 4) 根目录散落源文件（极简仓库）
    if out.is_empty() {
        let mut n = 0;
        for f in scan.files.iter().filter(|f| f.language.is_some()) {
            if !f.rel.contains('/') {
                n += 1;
            }
        }
        if n > 0 {
            out.push((
                "root".into(),
                "根目录源码".into(),
                "./".into(),
            ));
        }
    }

    // 按文件量/名字稳定排序，限制篇幅
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.truncate(12);
    out
}

pub(crate) fn derive_modules(scan: &RepoScan) -> Vec<(String, String)> {
    let mut map = std::collections::BTreeMap::new();
    for f in &scan.files {
        if let Some(lang) = &f.language {
            if matches!(lang.as_str(), "rust" | "typescript" | "python") {
                let top = f
                    .rel
                    .split('/')
                    .next()
                    .unwrap_or("root")
                    .to_string();
                if top != "docs" && top != "web" {
                    map.entry(top.clone()).or_insert(f.rel.clone());
                }
            }
        }
    }
    map.into_iter().collect()
}
