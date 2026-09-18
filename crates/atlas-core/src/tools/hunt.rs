//! Symbol-definition hunt and git history tools.

use super::RepoTools;
use super::glob::{clip, is_skipped_dir, is_text_extension, shell_glob_match};
use anyhow::{Result, anyhow};
use walkdir::WalkDir;

impl RepoTools {
    /// Exact-name definition hunt across common languages.
    pub(crate) fn find_defs(&self, name: &str, glob: Option<&str>, limit: usize) -> Result<String> {
        if name.trim().is_empty() {
            return Err(anyhow!("name must not be empty"));
        }
        let mut out: Vec<String> = Vec::new();
        for entry in WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_skipped_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = self.rel(entry.path());
            if rel.is_empty()
                || self.is_redacted(&rel)
                || !is_text_extension(&rel)
                || glob.is_some_and(|g| !shell_glob_match(g, &rel))
            {
                continue;
            }
            let meta = match std::fs::metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() as usize > self.max_file_bytes {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                if line_defines(line, name) {
                    out.push(format!("{rel}:{}: {}", i + 1, clip(line.trim())));
                    if out.len() >= limit {
                        break;
                    }
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        if out.is_empty() {
            return Ok(format!("(no definition of `{name}`)"));
        }
        Ok(out.join("\n"))
    }

    pub(crate) fn git_log(&self, path: Option<&str>, limit: usize) -> Result<String> {
        let mut cmd = std::process::Command::new("git");
        cmd.args([
            "--no-pager",
            "log",
            "--oneline",
            &format!("--max-count={limit}"),
        ]);
        if let Some(p) = path.filter(|p| !p.trim().is_empty()) {
            let full = self.resolve(p)?;
            cmd.arg("--");
            cmd.arg(full);
        }
        cmd.current_dir(&self.root);
        let out = cmd.output().map_err(|e| anyhow!("git log failed: {e}"))?;
        if !out.status.success() {
            return Ok(format!(
                "(git log unavailable: {})",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            return Ok("(no commits)".into());
        }
        Ok(text)
    }
}

/// True when `line` looks like a definition of exact symbol `name`.
fn line_defines(line: &str, name: &str) -> bool {
    let t = line.trim();
    if t.starts_with("//") || t.starts_with('#') || t.starts_with('*') || t.starts_with("/*") {
        return false;
    }
    let patterns = [
        format!("fn {name}"),
        format!("pub fn {name}"),
        format!("pub(crate) fn {name}"),
        format!("pub(super) fn {name}"),
        format!("struct {name}"),
        format!("pub struct {name}"),
        format!("enum {name}"),
        format!("pub enum {name}"),
        format!("trait {name}"),
        format!("pub trait {name}"),
        format!("type {name}"),
        format!("mod {name}"),
        format!("function {name}"),
        format!("class {name}"),
        format!("const {name}"),
        format!("export function {name}"),
        format!("export class {name}"),
        format!("export const {name}"),
        format!("export async function {name}"),
        format!("def {name}"),
        format!("class {name}:"),
        format!("interface {name}"),
        format!("export interface {name}"),
    ];
    patterns.iter().any(|p| {
        t.starts_with(p.as_str()) || t.contains(&format!("{p}(")) || t.contains(&format!("{p} "))
    })
}
