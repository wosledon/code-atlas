//! 提示词里的「代码地图」：范围内文件的关键声明（带行号）、manifest 声明的依赖、
//! 以及仓库里真实存在的命令。
//!
//! 这三段证据是「文档够深」的前提：模型据此知道有哪些文件、里面有什么符号、在哪一行，
//! 再用 `read_file` 精确读取，就不必把整个仓库塞进上下文，也不会编造符号。
//! 输出必须**确定性**（同一仓库得到同一字符串），因为 `page_fingerprint` 把证据算进
//! 指纹：证据抖动会导致每次运行都全量重新生成。

use super::evidence::{lang_of, run_commands, scoped_files};
use super::*;

use atlas_analyze::file_outline;

/// 每个页面最多展开多少个文件、每个文件最多列多少个声明。
/// 这段证据会随每一轮工具调用重复计费，宁可少列、让模型按需 `read_file`。
const MAX_FILES: usize = 14;
const MAX_ITEMS_PER_FILE: usize = 8;
/// 这一段在提示词里的字符上限，避免证据比正文还长。
const MAX_CHARS: usize = 4200;
/// 读取文件时只看前这么多字节（声明都在文件前部）。
const EXCERPT_BYTES: usize = 60_000;

/// 范围内文件的声明索引。大文件优先（更可能是核心实现）。
pub(crate) fn source_outline(scan: &RepoScan, page: &PlannedPage) -> String {
    let mut files = scoped_files(scan, page);
    files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.rel.cmp(&b.rel)));
    let mut out = String::from("\n## Source outline (declarations with line numbers)\n");
    out.push_str(
        "Use `read_file(path, start_line, end_line)` on the files you need and cite `path:line`.\n",
    );
    for f in files.iter().take(MAX_FILES) {
        if out.chars().count() > MAX_CHARS {
            out.push_str("- … (more files omitted; use list_files / grep)\n");
            break;
        }
        let source = read_file_excerpt(&f.path, EXCERPT_BYTES);
        let items = file_outline(f, &source);
        if items.is_empty() {
            out.push_str(&format!("- `{}` ({})\n", f.rel, lang_of(f)));
            continue;
        }
        let decls: Vec<String> = items
            .iter()
            .take(MAX_ITEMS_PER_FILE)
            .map(|i| format!("`{}` {} {}", i.line, i.kind, i.name))
            .collect();
        out.push_str(&format!(
            "- `{}` ({}, {} B): {}\n",
            f.rel,
            lang_of(f),
            f.size,
            decls.join(" · ")
        ));
    }
    if files.is_empty() {
        out.push_str("- (no source files in scope)\n");
    }
    out
}

/// 仓库里可直接运行的命令（来自 manifest 的脚本与约定）。
pub(crate) fn commands(scan: &RepoScan) -> String {
    let cmds = run_commands(scan);
    let mut out = String::from("\n## Commands that exist in this repository\n");
    if cmds.is_empty() {
        out.push_str("- (no manifest scripts detected)\n");
        return out;
    }
    for c in cmds {
        out.push_str(&format!("- `{c}`\n"));
    }
    out
}

/// manifest 里声明的依赖与脚本；让页面能说清「这个仓库依赖什么、怎么跑」。
pub(crate) fn manifest_summary(scan: &RepoScan) -> String {
    let mut out = String::from("\n## Manifests & declared dependencies\n");
    if scan.manifests.is_empty() {
        out.push_str("- (no manifest found)\n");
        return out;
    }
    let mut manifests: Vec<&PathBuf> = scan.manifests.iter().collect();
    // 指纹把证据算进去：manifest 顺序必须稳定，不能依赖扫描顺序
    manifests.sort_by_key(|m| rel_of(scan, m));
    for m in manifests.iter().take(12) {
        let rel = rel_of(scan, m);
        let Ok(text) = std::fs::read_to_string(m) else {
            out.push_str(&format!("- `{rel}` (unreadable)\n"));
            continue;
        };
        let name = m.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let detail = match name {
            "Cargo.toml" => cargo_summary(&text),
            "package.json" => package_summary(&text),
            "go.mod" => go_summary(&text),
            "pyproject.toml" => py_summary(&text),
            _ => String::new(),
        };
        out.push_str(&format!("- `{rel}`{detail}\n"));
    }
    if manifests.len() > 12 {
        out.push_str("- … (more manifests omitted)\n");
    }
    out
}

fn rel_of(scan: &RepoScan, path: &Path) -> String {
    path.strip_prefix(&scan.root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn cap(items: &[String], max: usize) -> String {
    let mut sorted: Vec<&String> = items.iter().collect();
    sorted.sort();
    sorted.dedup();
    let shown: Vec<String> = sorted.iter().take(max).map(|s| (*s).clone()).collect();
    let mut text = shown.join(", ");
    if sorted.len() > max {
        text.push_str(", …");
    }
    text
}

fn cargo_summary(text: &str) -> String {
    let mut deps = Vec::new();
    let mut package = String::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if section == "package" && line.starts_with("name") && package.is_empty() {
            if let Some((_, v)) = line.split_once('=') {
                package = v.trim().trim_matches('"').to_string();
            }
            continue;
        }
        if !section.contains("dependencies") {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            let key = key.trim().trim_matches('"');
            if !key.is_empty() && !key.starts_with('#') {
                deps.push(key.to_string());
            }
        }
    }
    let members = cargo_members(text);
    let mut parts = Vec::new();
    if !package.is_empty() {
        parts.push(format!("crate: {package}"));
    }
    if !members.is_empty() {
        parts.push(format!("workspace members: {}", members.join(", ")));
    }
    if !deps.is_empty() {
        parts.push(format!("deps: {}", cap(&deps, 40)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" · {}", parts.join(" · "))
    }
}

fn cargo_members(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_workspace = false;
    let mut collecting = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_workspace = line == "[workspace]";
            collecting = false;
            continue;
        }
        if !in_workspace {
            continue;
        }
        if let Some(rest) = line.strip_prefix("members") {
            collecting = true;
            push_quoted(rest, &mut out);
            if rest.contains(']') {
                collecting = false;
            }
        } else if collecting {
            push_quoted(line, &mut out);
            if line.contains(']') {
                collecting = false;
            }
        }
    }
    out
}

fn push_quoted(text: &str, out: &mut Vec<String>) {
    for part in text.split('"').skip(1).step_by(2) {
        if !part.is_empty() {
            out.push(part.to_string());
        }
    }
}

fn package_summary(text: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return String::new();
    };
    let keys = |k: &str| -> Vec<String> {
        value
            .get(k)
            .and_then(|v| v.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    let mut parts = Vec::new();
    if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
        parts.push(format!("name: {name}"));
    }
    let scripts = keys("scripts");
    if !scripts.is_empty() {
        parts.push(format!("scripts: {}", cap(&scripts, 30)));
    }
    let deps = keys("dependencies");
    if !deps.is_empty() {
        parts.push(format!("deps: {}", cap(&deps, 30)));
    }
    let dev = keys("devDependencies");
    if !dev.is_empty() {
        parts.push(format!("devDeps: {}", cap(&dev, 30)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" · {}", parts.join(" · "))
    }
}

fn go_summary(text: &str) -> String {
    let mut deps = Vec::new();
    let mut in_require = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("require (") || line == "require (" {
            in_require = true;
            continue;
        }
        if in_require {
            if line.starts_with(')') {
                in_require = false;
                continue;
            }
            if let Some(first) = line.split_whitespace().next() {
                deps.push(first.to_string());
            }
        }
    }
    if deps.is_empty() {
        String::new()
    } else {
        format!(" · deps: {}", cap(&deps, 30))
    }
}

fn py_summary(text: &str) -> String {
    let mut deps = Vec::new();
    let mut in_deps = false;
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            in_deps = false;
            continue;
        }
        if line.starts_with("dependencies") {
            in_deps = true;
            push_quoted(line, &mut deps);
            if line.contains(']') {
                in_deps = false;
            }
            continue;
        }
        if in_deps {
            push_quoted(line, &mut deps);
            if line.contains(']') {
                in_deps = false;
            }
        } else if section.ends_with("dependencies")
            && let Some((key, _)) = line.split_once('=')
        {
            let key = key.trim().trim_matches('"');
            if !key.is_empty() && !key.starts_with('#') {
                deps.push(key.to_string());
            }
        }
    }
    if deps.is_empty() {
        String::new()
    } else {
        format!(" · deps: {}", cap(&deps, 30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-outline-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n\n[dependencies]\nanyhow = \"1\"\nserde = { version = \"1\" }\n\n[dev-dependencies]\ntempfile = \"3\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/lib.rs"),
            "pub struct Demo;\n\nimpl Demo {\n    pub fn start(&self) -> u8 {\n        1\n    }\n}\n\npub fn helper() {}\n",
        )
        .unwrap();
        dir
    }

    fn page(kind: &str) -> PlannedPage {
        PlannedPage {
            rel_path: "03-架构/架构总览.md".into(),
            title: "架构总览".into(),
            page_type: kind.into(),
            description: "分层、模块边界与依赖方向".into(),
            tags: vec![],
            module: None,
            focus: "architecture".into(),
        }
    }

    #[test]
    fn evidence_sections_carry_symbols_manifests_and_commands() {
        let root = temp_repo("evidence");
        let scan = scan_repo(&root).expect("scan");

        let outline = source_outline(&scan, &page("Architecture"));
        assert!(outline.contains("`src/lib.rs`"), "{outline}");
        // 行号来自真实文件，模型据此 read_file
        assert!(outline.contains("`1` struct Demo"), "{outline}");
        assert!(outline.contains("`4` fn start"), "{outline}");
        assert!(outline.contains("`9` fn helper"), "{outline}");

        let manifests = manifest_summary(&scan);
        for dep in ["anyhow", "serde", "tempfile", "crate: demo"] {
            assert!(manifests.contains(dep), "missing {dep} in {manifests}");
        }

        let commands = commands(&scan);
        assert!(commands.contains("cargo"), "{commands}");

        // 指纹依赖证据的确定性：同一仓库两次调用必须完全一致
        assert_eq!(outline, source_outline(&scan, &page("Architecture")));
        assert_eq!(manifests, manifest_summary(&scan));
        let _ = fs::remove_dir_all(&root);
    }
}
