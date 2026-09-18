//! File listing / reading / grep / tree — the core evidence tools.

use super::RepoTools;
use super::compress;
use super::glob::{clip, is_skipped_dir, is_text_extension, shell_glob_match};
use anyhow::{Result, anyhow};
use std::path::Path;
use std::sync::Arc;
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
        let rel = self.rel(&full);
        // One disk read per file per run: any window of a file already read is
        // served from the snapshot, so a shifted or overlapping range costs
        // nothing. The repo is frozen for the run, so this cannot go stale.
        let text: Arc<str> = match self.cache_file(&rel) {
            Some(text) => text,
            None => {
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
                let text: Arc<str> = Arc::from(String::from_utf8_lossy(&bytes).into_owned());
                self.cache_file_put(rel.clone(), text.clone());
                text
            }
        };
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
        // ~4 字节/token 够用的压缩统计：压缩后的文本才是进对话、进 token 账单的那份。
        let mut out = format!("// {path} lines {from}-{to} of {total}\n");
        if to < total {
            out.push_str(&format!(
                "// … not the whole file: continue with start_line={}\n",
                to + 1
            ));
        }
        let clipped: Vec<String> = lines[from - 1..to].iter().map(|l| clip(l)).collect();
        let window: Vec<&str> = clipped.iter().map(String::as_str).collect();
        // 缩进外提（可逆、按块、按语言开闸）：缩进占 tsx/rs/json 字数的 10–24%，
        // 按块外提实测能省 6.2%（整窗只有 2.6%——真实读窗口常从 0 缩进行开始）。
        // 语言白名单挡住缩进即语法的语言；跨行字符串内的行由 `string_lines` 逐行标出、
        // 块会绕开它们（Go 原生字符串里的 SQL、JS 模板字符串里的片段都是内容）。
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let joined = window.join("\n");
        let blocks = if compress::hoist_allowed(&ext, &joined) {
            let blocked = compress::string_lines(&window, &ext);
            compress::indent_blocks(&window, &blocked)
        } else {
            Vec::new()
        };
        let saved: usize = blocks.iter().map(|b| b.saved()).sum();
        if saved > 0 {
            self.note_saved(saved);
        }
        // 行号栏收窄到实际需要的宽度：固定 `{:>5}` 在 3 位数文件上白付 2 字符/行，
        // 实测那占了全仓字符的 20%（收窄后省 5.8%）。
        let width = to.to_string().len();
        let mut rows: Vec<(usize, String)> = Vec::with_capacity(window.len());
        // 记号与行号同一坐标系（**绝对行号**）：缓冲后统一按行号排序吐出，
        // 所以这里必须写 `from + i`——用窗口下标会让记号整体前移 `from` 行。
        let mut markers: Vec<(usize, String)> = Vec::new();
        let mut next = 0usize;
        for (i, line) in window.iter().enumerate() {
            if let Some(block) = blocks.get(next).filter(|b| b.start == i) {
                markers.push((from + i, compress::block_start_marker(block.indent)));
            }
            let in_block = blocks.get(next).filter(|b| i >= b.start && i < b.end);
            let body = match in_block {
                Some(b) => compress::strip_indent(line, b.indent),
                None => *line,
            };
            rows.push((from + i, body.to_string()));
            if let Some(b) = blocks.get(next)
                && b.end == i + 1
            {
                // 结束记号落在块的**后一行**：它要排在最后一行之后，否则末行会
                // 显示在「块外」，读者按记号补回缩进时就漏掉它。
                markers.push((from + i + 1, compress::BLOCK_END.to_string()));
                next += 1;
            }
        }
        // 重复行折叠（可逆）：生成代码里成片的 `_ = fileDescriptor`、数据里成片的同形
        // 条目才触发；保留行仍用**原始行号**，所以 `path:line` 引用不会失效，跳号本身
        // 就是「这里折了 N 行」的信号。
        let (rows, folded) = compress::fold_identical_runs(rows);
        if folded > 0 {
            self.note_saved(folded);
        }
        // 记号行不占行号：给了行号会让下面每行偏 1，而模型正是按行号引用的。
        for (i, body) in rows {
            while let Some((at, marker)) = markers.first()
                && *at <= i
            {
                out.push_str(marker);
                out.push('\n');
                markers.remove(0);
            }
            out.push_str(&format!("{i:>width$}| {body}\n"));
        }
        for (_, marker) in markers {
            out.push_str(&marker);
            out.push('\n');
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
