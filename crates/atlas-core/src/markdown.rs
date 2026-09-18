use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Written at the top of the body while a page is still being streamed to disk.
/// A file that carries it is a draft — not a page — so reuse and reindexing skip
/// it, and the finished write removes it.
pub(crate) const DRAFT_MARKER: &str = "<!-- atlas:draft -->";

#[derive(Debug, Clone, PartialEq)]
pub struct FrontMatter {
    pub fields: BTreeMap<String, String>,
}

impl FrontMatter {
    pub fn new(kind: &str, title: &str, description: &str, tags: &[&str]) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("type".into(), kind.into());
        fields.insert("title".into(), title.into());
        fields.insert("description".into(), description.into());
        fields.insert(
            "tags".into(),
            format!("[{}]", tags.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ")),
        );
        Self { fields }
    }

    pub fn render(&self) -> String {
        let mut s = String::from("---\n");
        for (k, v) in &self.fields {
            s.push_str(&format!("{k}: {v}\n"));
        }
        s.push_str("---\n");
        s
    }
}

pub fn write_page(path: &Path, fm: &FrontMatter, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Normalise the body so writing is idempotent: an LLM body usually ends
    // with a newline and a reused body comes back from disk, yet both must
    // produce the same bytes or the body hash (and the chunk split) would
    // change on every run.
    let content = format!("{}{}\n", fm.render(), body.trim_start_matches('\n').trim_end());
    atomic_write(path, content.as_bytes())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp-atlas");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn hash_file(path: &Path) -> String {
    let data = std::fs::read(path).unwrap_or_default();
    crate::sha256_hex(&data)
}

pub fn link_check(atlas_root: &Path) -> Vec<String> {
    let mut broken = Vec::new();
    if !atlas_root.exists() {
        return broken;
    }
    for entry in walkdir::WalkDir::new(atlas_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let mut in_fence = false;
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            for target in line_targets(line) {
                let target = target.split('#').next().unwrap_or(&target).to_string();
                if target.is_empty() {
                    continue;
                }
                let resolved = entry.path().parent().unwrap_or(atlas_root).join(&target);
                if !resolved.exists() {
                    broken.push(format!("{} -> {}", entry.path().display(), target));
                }
            }
        }
    }
    broken
}

/// Link destinations in one markdown line: `[text](dest)` and
/// `[label]: dest`, ignoring inline code spans.
fn line_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = strip_inline_code(line);
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let end = after.find(')').unwrap_or(after.len());
        let dest = after[..end].trim();
        // drop an optional link title: `path "Title"`
        let dest = dest.split_whitespace().next().unwrap_or(dest);
        let dest = dest.trim_matches(|c| c == '<' || c == '>');
        if is_relative_dest(dest) {
            out.push(dest.to_string());
        }
        rest = after[end.min(after.len())..].to_string();
    }
    let mut rest = strip_inline_code(line);
    while let Some(i) = rest.find("]:") {
        let after = &rest[i + 2..];
        let dest = after.trim();
        let dest = dest.split_whitespace().next().unwrap_or(dest);
        if is_relative_dest(dest) {
            out.push(dest.to_string());
        }
        rest = after.to_string();
    }
    out
}

fn is_relative_dest(dest: &str) -> bool {
    if dest.is_empty() || dest.starts_with('#') || dest.starts_with('/') {
        return false;
    }
    if dest.starts_with("./") || dest.starts_with("../") {
        return true;
    }
    // `foo/bar.md` — relative, but skip schemes (http:, mailto:, tel:) and bare anchors
    !dest.contains(':')
}

fn strip_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            out.push(ch);
        }
    }
    out
}
