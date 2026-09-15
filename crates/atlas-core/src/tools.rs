//! Read-only repository tools handed to the LLM while it writes a wiki page.
//!
//! Giving the model `read_file` / `list_files` / `grep` lets a page be generated
//! from a *targeted* slice of the repository instead of pasting the whole scan
//! into every prompt. Every path is confined to the repository root and filtered
//! by `privacy.redact_paths` / `privacy.max_file_bytes`.

use anyhow::{anyhow, Result};
use atlas_llm::ToolSpec;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const DEFAULT_MAX_RESULTS: usize = 40;
const MAX_TOOL_RESULTS: usize = 200;
const MAX_LINE_CHARS: usize = 400;

pub struct RepoTools {
    root: PathBuf,
    redact: Vec<String>,
    max_file_bytes: usize,
}

impl RepoTools {
    pub fn new(root: &Path, redact: Vec<String>, max_file_bytes: usize) -> Result<Self> {
        let root = root
            .canonicalize()
            .map_err(|e| anyhow!("cannot canonicalize repo root {}: {e}", root.display()))?;
        Ok(Self {
            root,
            redact,
            max_file_bytes: max_file_bytes.max(1024),
        })
    }

    pub fn specs() -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list_files".into(),
                description: "List files tracked in the repository, optionally filtered by a glob \
                    such as \"crates/**/*.rs\" or \"web/src/**\". Use it to discover structure \
                    before reading individual files."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "glob": { "type": "string", "description": "glob filter, e.g. crates/atlas-core/src/**/*.rs" },
                        "limit": { "type": "integer", "description": "max entries to return (default 60)" }
                    }
                }),
            },
            ToolSpec {
                name: "read_file".into(),
                description: "Read a UTF-8 text file from the repository. Prefer this over guessing: \
                    open the real source before describing behaviour. Optionally restrict to a line range."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "repository-relative path, e.g. crates/atlas-store/src/lib.rs" },
                        "start_line": { "type": "integer", "description": "1-based first line (default 1)" },
                        "end_line": { "type": "integer", "description": "1-based last line (default: end of file)" }
                    },
                    "required": ["path"]
                }),
            },
            ToolSpec {
                name: "grep".into(),
                description: "Case-insensitive substring search across repository text files. Returns \
                    file:line matches. Use it to find where a symbol, config key or message is used."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "literal substring to search for" },
                        "glob": { "type": "string", "description": "optional glob filter, e.g. **/*.rs" },
                        "max_results": { "type": "integer", "description": "max matches (default 40)" }
                    },
                    "required": ["pattern"]
                }),
            },
        ]
    }

    /// Execute one tool call. `args` is the raw JSON arguments string produced by
    /// the model; malformed JSON is reported back as an error the model can fix.
    pub fn call(&self, name: &str, args: &str) -> Result<String> {
        let args: Value = if args.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(args)
                .map_err(|e| anyhow!("arguments must be a JSON object: {e}"))?
        };
        let glob = args.get("glob").and_then(|v| v.as_str()).map(str::to_string);
        let limit = args
            .get("limit")
            .or_else(|| args.get("max_results"))
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_RESULTS as u64)
            .min(MAX_TOOL_RESULTS as u64) as usize;

        match name {
            "list_files" => self.list_files(glob.as_deref(), limit),
            "read_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("read_file requires a \"path\" argument"))?;
                let start = args.get("start_line").and_then(|v| v.as_u64());
                let end = args.get("end_line").and_then(|v| v.as_u64());
                self.read_file(path, start, end)
            }
            "grep" => {
                let pattern = args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("grep requires a \"pattern\" argument"))?;
                self.grep(pattern, glob.as_deref(), limit)
            }
            other => Err(anyhow!("unknown tool `{other}`")),
        }
    }

    fn list_files(&self, glob: Option<&str>, limit: usize) -> Result<String> {
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

    fn read_file(&self, path: &str, start: Option<u64>, end: Option<u64>) -> Result<String> {
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
        let to = end.unwrap_or(total as u64).max(1) as usize;
        if from > total {
            return Err(anyhow!("start_line {from} is past the end of `{path}` ({total} lines)"));
        }
        let to = to.min(total).max(from);
        let mut out = format!("// {path} lines {from}-{to} of {total}\n");
        for (i, line) in lines[from - 1..to].iter().enumerate() {
            out.push_str(&format!("{:>5}| {}\n", from + i, clip(line)));
        }
        Ok(out)
    }

    fn grep(&self, pattern: &str, glob: Option<&str>, limit: usize) -> Result<String> {
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

    /// Resolve a repo-relative path and guarantee it cannot escape the repo.
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let trimmed = path.trim().trim_start_matches("./");
        if trimmed.is_empty() {
            return Err(anyhow!("path must not be empty"));
        }
        let candidate = self.root.join(trimmed);
        let canonical = candidate
            .canonicalize()
            .map_err(|e| anyhow!("cannot open `{path}`: {e}"))?;
        if !canonical.starts_with(&self.root) {
            return Err(anyhow!("path `{path}` is outside the repository"));
        }
        let rel = self.rel(&canonical);
        if self.is_redacted(&rel) {
            return Err(anyhow!("path `{path}` is excluded by privacy.redact_paths"));
        }
        Ok(canonical)
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }

    fn is_redacted(&self, rel: &str) -> bool {
        self.redact.iter().any(|p| shell_glob_match(p, rel))
    }
}

fn is_skipped_dir(path: &Path) -> bool {
    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    path.is_dir()
        && matches!(
            name.as_str(),
            ".git" | "node_modules" | "target" | "dist" | ".venv" | "venv" | "__pycache__" | ".atlas-data"
        )
}

fn is_text_extension(rel: &str) -> bool {
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "rs" | "toml" | "json" | "md" | "txt" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java"
            | "c" | "h" | "cpp" | "hpp" | "cs" | "rb" | "php" | "sh" | "ps1" | "bat" | "yml" | "yaml"
            | "sql" | "css" | "scss" | "html" | "vue" | "kt" | "swift" | "gradle" | "mod" | "sum"
            | "cfg" | "ini" | "xml" | "proto" | "mjs" | "cjs" | "lock" | "env" | "gitignore"
    )
}

fn clip(line: &str) -> String {
    let t = line.trim_end();
    if t.chars().count() <= MAX_LINE_CHARS {
        return t.to_string();
    }
    let mut s: String = t.chars().take(MAX_LINE_CHARS).collect();
    s.push('…');
    s
}

/// Glob matcher with `**` (any depth), `*` (within one segment) and `?`.
pub fn shell_glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (
        pattern.replace('\\', "/").chars().collect(),
        text.replace('\\', "/").chars().collect(),
    );
    glob_chars(&p, &t)
}

fn glob_chars(pat: &[char], txt: &[char]) -> bool {
    if pat.is_empty() {
        return txt.is_empty();
    }
    match pat[0] {
        '*' if pat.len() > 1 && pat[1] == '*' => {
            let rest = &pat[2..];
            // `**/foo` also matches a top-level `foo`.
            if rest.first() == Some(&'/') && glob_chars(&rest[1..], txt) {
                return true;
            }
            (0..=txt.len()).any(|i| glob_chars(rest, &txt[i..]))
        }
        '*' => (0..=txt.len())
            .take_while(|i| *i == 0 || txt[i - 1] != '/')
            .any(|i| glob_chars(&pat[1..], &txt[i..])),
        '?' => match txt.first() {
            Some(c) if *c != '/' => glob_chars(&pat[1..], &txt[1..]),
            _ => false,
        },
        c => match txt.first() {
            Some(t) if *t == c => glob_chars(&pat[1..], &txt[1..]),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(root: &Path) -> RepoTools {
        RepoTools::new(root, vec!["**/.env".into(), "**/*.pem".into()], 262_144).unwrap()
    }

    #[test]
    fn glob_matches_expected_shapes() {
        assert!(shell_glob_match("**/*.rs", "crates/atlas-core/src/lib.rs"));
        assert!(shell_glob_match("crates/**/*.rs", "crates/atlas-core/src/lib.rs"));
        assert!(shell_glob_match("**/.env", ".env"));
        assert!(shell_glob_match("**/.env", "web/.env"));
        assert!(!shell_glob_match("*.rs", "src/lib.rs"));
        assert!(shell_glob_match("src/*.rs", "src/lib.rs"));
        assert!(!shell_glob_match("src/?b.rs", "src/abc.rs"));
    }

    #[test]
    fn read_file_rejects_escapes_and_redacted_paths() {
        let dir = std::env::temp_dir().join(format!("atlas-tools-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join(".env"), "SECRET=1\n").unwrap();
        let t = tools(&dir);

        assert!(t.read_file("src/lib.rs", None, None).unwrap().contains("fn main"));
        assert!(t.read_file("../../etc/hosts", None, None).is_err());
        assert!(t.read_file("C:\\Windows\\System32\\drivers\\etc\\hosts", None, None).is_err());
        assert!(t.read_file(".env", None, None).is_err());
        assert!(t.call("read_file", "{\"path\":\"src/lib.rs\",\"start_line\":1,\"end_line\":1}")
            .unwrap()
            .contains("1| fn main"));

        let hits = t.call("grep", "{\"pattern\":\"MAIN\",\"glob\":\"**/*.rs\"}").unwrap();
        assert!(hits.contains("src/lib.rs:1"), "got {hits}");
        // redacted file is never surfaced by grep either
        assert!(t.call("grep", "{\"pattern\":\"SECRET\"}").unwrap().starts_with("(no matches"));
        assert!(t.call("read_file", "not json at all").is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
