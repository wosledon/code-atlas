use super::*;

use super::plan::plan_pages;
use super::plan_modules::detect_code_modules;
use super::outline::{
    build_outline_cache, commands, manifest_summary, source_outline_from_cache, OutlineCache,
};

/// Evidence fragments identical for every page in one run (built once).
pub(crate) struct EvidenceShared {
    header: String,
    docs_section: String,
    outlines: OutlineCache,
}

/// Build the shared evidence header + wiki index + source-outline cache once per run.
pub(crate) fn build_evidence_shared(scan: &RepoScan, plan: &[PlannedPage]) -> EvidenceShared {
    let mut header = String::new();
    header.push_str("## Repository\n");
    header.push_str(&format!("root: {}\n", scan.root.display()));
    header.push_str(&format!(
        "source files: {} · languages: {}\n",
        scan.files.iter().filter(|f| f.language.is_some()).count(),
        if scan.languages.is_empty() {
            "unknown".to_string()
        } else {
            scan.languages.join(", ")
        }
    ));
    if let Some(readme) = &scan.readme_excerpt {
        header.push_str("\n## README excerpt\n");
        header.push_str(&readme.chars().take(900).collect::<String>());
        header.push('\n');
    }
    header.push_str(&manifest_summary(scan));
    header.push_str(&commands(scan));
    header.push_str("\n## Modules discovered\n");
    for (key, label, hint) in detect_code_modules(scan).iter().take(12) {
        let files = scan
            .files
            .iter()
            .filter(|f| f.language.is_some() && f.rel.starts_with(hint.as_str()))
            .count();
        header.push_str(&format!("- {label} (`{key}` → `{hint}`) · {files} source files\n"));
    }

    // Prefer the live plan; fall back to a structural plan preview.
    let owned;
    let docs: &[PlannedPage] = if plan.is_empty() {
        owned = plan_pages(scan, "plan", None);
        &owned
    } else {
        plan
    };
    let mut docs_section = String::from("\n## Documents this wiki will contain\n");
    for p in docs.iter().take(12) {
        docs_section.push_str(&format!("- [{}]({}) — {}\n", p.title, p.rel_path, p.description));
    }
    EvidenceShared {
        header,
        docs_section,
        outlines: build_outline_cache(scan),
    }
}

/// Compact repository map handed to the model together with the file/read
/// tools. It intentionally contains *no* large source dumps: the model pulls the
/// code it needs through `read_file` / `grep`, which keeps prompts small and
/// lets pages cover the whole repository instead of a handful of sampled files.
/// Prefer [`build_evidence_with_shared`] in run paths; this helper is for tests
/// and one-off previews.
#[allow(dead_code)]
pub(crate) fn build_evidence(scan: &RepoScan, page: &PlannedPage) -> String {
    let shared = build_evidence_shared(scan, &plan_pages(scan, "plan", None));
    build_evidence_with_shared(scan, page, &shared)
}

/// Per-page evidence: shared header + module-scoped file list + symbol outline.
pub(crate) fn build_evidence_with_shared(
    scan: &RepoScan,
    page: &PlannedPage,
    shared: &EvidenceShared,
) -> String {
    let mut out = String::with_capacity(shared.header.len() + 2048);
    out.push_str(&shared.header);
    let module_root = module_scope(page);
    let relevant: Vec<&SourceFile> = scan
        .files
        .iter()
        .filter(|f| f.language.is_some())
        .filter(|f| module_root.as_deref().is_none_or(|r| f.rel.starts_with(r)))
        .collect();
    out.push_str(&format!(
        "\n## Source files in scope ({})\n",
        if module_root.is_some() { "module" } else { "whole repository" }
    ));
    for (listed, f) in relevant.iter().enumerate() {
        // The list is orientation, not content: the outline below names the
        // files that matter, and `list_files` has the rest.
        if listed >= 60 {
            out.push_str("- … (truncated; use list_files/grep for the rest)\n");
            break;
        }
        out.push_str(&format!("- {} ({}, {} B)\n", f.rel, lang_of(f), f.size));
    }
    out.push_str(&source_outline_from_cache(&shared.outlines, scan, page));
    out.push_str(&shared.docs_section);
    out
}

/// Path prefix a module page is responsible for; `None` = whole repository.
pub(crate) fn module_scope(page: &PlannedPage) -> Option<String> {
    page.module
        .as_deref()
        .filter(|m| m.contains('/') || *m == "src")
        .map(|m| format!("{m}/"))
}

/// Fingerprint of everything a page depends on: the repository state (files and
/// snippet sizes), the evidence built for it, and the `salt` describing how it
/// will be generated (prompt text, model, generation knobs). Pages whose
/// fingerprint is unchanged since the last run are reused verbatim.
pub(crate) fn page_fingerprint(
    scan: &RepoScan,
    page: &PlannedPage,
    evidence: &str,
    salt: &str,
) -> String {
    let module_root = module_scope(page);
    let mut feed = String::new();
    feed.push_str(salt);
    feed.push('\n');
    feed.push_str(&page.focus);
    feed.push('\n');
    feed.push_str(evidence);
    feed.push('\n');
    for f in scan.files.iter() {
        if let Some(root) = module_root.as_deref()
            && !f.rel.starts_with(root) {
                continue;
            }
        feed.push_str(&format!("{}:{}\n", f.rel, f.size));
    }
    sha256_hex(feed.as_bytes())
}

/// Drop the YAML front matter so a reused page can be re-chunked/re-verified
/// with exactly the same body the model originally produced.
pub(crate) fn strip_front_matter(text: &str) -> String {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !t.starts_with("---") {
        return text.to_string();
    }
    let rest = &t[3..];
    if let Some(end) = rest.find("\n---") {
        return rest[end + 4..].trim_start_matches(['\r', '\n']).to_string();
    }
    text.to_string()
}

pub(crate) fn read_page_body(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let body = strip_front_matter(&text);
    // A draft (a page the model is still writing, or one a killed run left
    // behind) must never be reused as if the model had produced it.
    if body.trim().is_empty() || body.contains(markdown::DRAFT_MARKER) {
        None
    } else {
        Some(body)
    }
}

/// Sources whose `rel` is under the page's module scope (or the whole repo).
pub(crate) fn lang_of(f: &SourceFile) -> &str {
    f.language.as_deref().unwrap_or("-")
}

pub(crate) fn scoped_files<'a>(scan: &'a RepoScan, page: &PlannedPage) -> Vec<&'a SourceFile> {
    let scope = module_scope(page);
    scan.files
        .iter()
        .filter(|f| f.language.is_some())
        .filter(|f| scope.as_deref().is_none_or(|r| f.rel.starts_with(r)))
        .collect()
}

/// Top-level directories with their source-file counts.
pub(crate) fn top_dirs(scan: &RepoScan) -> Vec<(String, usize)> {
    let mut map: std::collections::BTreeMap<String, usize> = Default::default();
    for f in scan.files.iter().filter(|f| f.language.is_some()) {
        let top = match f.rel.split_once('/') {
            Some((d, _)) => d.to_string(),
            None => ".".to_string(),
        };
        *map.entry(top).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = map.into_iter().collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.1));
    v
}

pub(crate) const ENTRY_NAMES: &[&str] = &[
    "main.rs",
    "lib.rs",
    "App.tsx",
    "main.ts",
    "main.tsx",
    "index.ts",
    "index.tsx",
    "main.py",
    "__main__.py",
    "app.py",
    "main.go",
    "Main.java",
    "Program.cs",
];

pub(crate) fn entry_files(scan: &RepoScan) -> Vec<&SourceFile> {
    scan.files
        .iter()
        .filter(|f| f.language.is_some())
        .filter(|f| {
            let name = f.rel.rsplit('/').next().unwrap_or(&f.rel);
            ENTRY_NAMES.contains(&name)
        })
        .take(20)
        .collect()
}

/// Real commands discovered from the repository's manifests.
pub(crate) fn run_commands(scan: &RepoScan) -> Vec<String> {
    let mut cmds = Vec::new();
    if scan.manifests.iter().any(|p| p.ends_with("package.json")) {
        let pkg = scan.root.join("package.json");
        let pkg = if pkg.exists() { Some(pkg) } else { scan.manifests.iter().find(|p| p.ends_with("package.json")).cloned() };
        if let Some(p) = pkg
            && let Ok(text) = std::fs::read_to_string(&p)
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
                    && let Some(scripts) = v.get("scripts").and_then(|s| s.as_object()) {
                        for k in scripts.keys() {
                            cmds.push(format!("npm run {k}"));
                        }
                    }
    }
    if scan.manifests.iter().any(|p| p.ends_with("Cargo.toml")) {
        cmds.push("cargo build".into());
        cmds.push("cargo test --workspace".into());
        cmds.push("cargo clippy --workspace --all-targets".into());
    }
    if scan.manifests.iter().any(|p| p.ends_with("go.mod")) {
        cmds.push("go build ./...".into());
        cmds.push("go test ./...".into());
    }
    if scan.manifests.iter().any(|p| p.ends_with("pyproject.toml")) {
        cmds.push("python -m pytest".into());
    }
    cmds
}

pub(crate) fn todo_hotspots(scan: &RepoScan) -> Vec<(String, usize)> {
    let mut hits: Vec<(String, usize)> = Vec::new();
    for f in scan.files.iter().filter(|f| f.language.is_some()).take(120) {
        let Some(text) = std::fs::read_to_string(&f.path).ok() else {
            continue;
        };
        let n = text
            .lines()
            .filter(|l| l.contains("TODO") || l.contains("FIXME") || l.contains("XXX"))
            .count();
        if n > 0 {
            hits.push((f.rel.clone(), n));
        }
    }
    hits.sort_by_key(|b| std::cmp::Reverse(b.1));
    hits.truncate(15);
    hits
}
