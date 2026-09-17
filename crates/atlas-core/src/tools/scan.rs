//! File listing / reading / grep / tree — the core evidence tools.

use super::RepoTools;
use super::glob::{clip, is_skipped_dir, is_text_extension, shell_glob_match};
use anyhow::{Result, anyhow};
use std::path::Path;
use walkdir::WalkDir;

/// Lines returned by an un-ranged `read_file`; the caller can continue from
/// there. Sized so a full window still fits in the client-side result budget.
const READ_WINDOW_LINES: usize = 120;

impl RepoTools {
    pub(crate) fn list_files(&self, glob: Option<&str>, limit: usize) -> Result<String> {
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
            if rel.is_empty() || self.is_redacted(&rel) {
                continue;
            }
            if let Some(g) = glob
                && !shell_glob_match(g, &rel)
            {
                continue;
            }
            out.push(rel);
            if out.len() >= limit {
                break;
            }
        }
        out.sort();
        if out.is_empty() {
            return Ok("(no matching files)".into());
        }
        Ok(out.join("\n"))
    }

    pub(crate) fn read_file(
        &self,
        path: &str,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<String> {
        let full = self.resolve(path)?;
        let meta = std::fs::metadata(&full)?;
        if meta.len() as usize > self.max_file_bytes {
            return Err(anyhow!(
                "file is {} bytes which exceeds the {}-byte limit",
                meta.len(),
                self.max_file_bytes
            ));
        }
        let bytes = std::fs::read(&full)?;
        if bytes.iter().take(1024).any(|b| *b == 0) {
            return Err(anyhow!("refusing to read binary file `{path}`"));
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        let from = start.unwrap_or(1).max(1) as usize;
        if from > total {
            return Err(anyhow!(
                "start_line {from} is past the end of `{path}` ({total} lines)"
            ));
        }
        // Un-ranged reads return one window and say how much is left: a whole
        // 2000-line file would be truncated mid-way anyway, and the model can
        // aim the next read instead of paying for a blind one.
        let to = match end {
            Some(e) => (e.max(1) as usize).max(from).min(total),
            None => (from + READ_WINDOW_LINES - 1).min(total),
        };
        let mut out = format!("// {path} lines {from}-{to} of {total}\n");
        if to < total {
            out.push_str(&format!(
                "// … not the whole file: continue with start_line={}\n",
                to + 1
            ));
        }
        for (i, line) in lines[from - 1..to].iter().enumerate() {
            out.push_str(&format!("{:>5}| {}\n", from + i, clip(line)));
        }
        Ok(out)
    }

    pub(crate) fn grep(&self, pattern: &str, glob: Option<&str>, limit: usize) -> Result<String> {
        let needle = pattern.to_lowercase();
        if needle.is_empty() {
            return Err(anyhow!("pattern must not be empty"));
        }
        let mut out: Vec<String> = Vec::new();
        let mut scanned = 0usize;
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
            let bytes = match std::fs::read(entry.path()) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if bytes.iter().take(1024).any(|b| *b == 0) {
                continue;
            }
            scanned += 1;
            let text = String::from_utf8_lossy(&bytes);
            for (i, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    out.push(format!("{rel}:{}: {}", i + 1, clip(line)));
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
            return Ok(format!("(no matches for `{pattern}` in {scanned} files)"));
        }
        let truncated = out.len() >= limit;
        let mut body = out.join("\n");
        if truncated {
            body.push_str("\n(more matches omitted)");
        }
        Ok(body)
    }

    pub(crate) fn list_tree(&self, path: &str, depth: usize, limit: usize) -> Result<String> {
        let base = if path.trim().is_empty() || path == "." {
            self.root.clone()
        } else {
            self.resolve(path)?
        };
        if !base.is_dir() {
            return Err(anyhow!("`{path}` is not a directory"));
        }
        let mut out: Vec<String> = Vec::new();
        self.walk_tree(&base, "", 1, depth, limit, &mut out);
        if out.is_empty() {
            return Ok("(empty)".into());
        }
        let mut body = out.join("\n");
        if out.len() >= limit {
            body.push_str("\n(truncated)");
        }
        Ok(body)
    }

    fn walk_tree(
        &self,
        dir: &Path,
        prefix: &str,
        depth: usize,
        max_depth: usize,
        limit: usize,
        out: &mut Vec<String>,
    ) {
        if depth > max_depth || out.len() >= limit {
            return;
        }
        let mut entries: Vec<(String, bool)> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if name.starts_with('.') || is_skipped_dir(&e.path()) {
                        return None;
                    }
                    let rel = self.rel(&e.path());
                    if !rel.is_empty() && self.is_redacted(&rel) {
                        return None;
                    }
                    Some((name, is_dir))
                })
                .collect(),
            Err(_) => return,
        };
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (name, is_dir) in entries {
            if out.len() >= limit {
                return;
            }
            let mark = if is_dir { "/" } else { "" };
            out.push(format!("{prefix}{name}{mark}"));
            if is_dir {
                self.walk_tree(
                    &dir.join(&name),
                    &format!("{prefix}  "),
                    depth + 1,
                    max_depth,
                    limit,
                    out,
                );
            }
        }
    }
}
