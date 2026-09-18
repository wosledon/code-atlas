//! Read-only repository tools handed to the LLM while it writes a wiki page.
//!
//! Tools let a page be generated from a *targeted* slice of the repository
//! instead of pasting the whole scan into every prompt. Paths stay confined
//! to the repo root and are filtered by privacy rules.

mod cache;
mod compress;
mod glob;
mod hunt;
mod scan;

use anyhow::{Result, anyhow};
use atlas_llm::ToolSpec;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub use cache::CacheStats;
pub use glob::shell_glob_match;

pub const DEFAULT_MAX_RESULTS: usize = 40;
const MAX_TOOL_RESULTS: usize = 200;

pub struct RepoTools {
    pub(crate) root: PathBuf,
    pub(crate) redact: Vec<String>,
    pub(crate) max_file_bytes: usize,
    cache: cache::ToolCache,
    /// Characters the compression pipeline kept out of the conversation.
    compressed_saved: AtomicUsize,
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
            cache: cache::ToolCache::default(),
            compressed_saved: AtomicUsize::new(0),
        })
    }

    /// What the snapshot cache did so far (hits, misses, bytes held).
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Characters the compression pipeline removed from tool output this run.
    pub fn compression_saved(&self) -> usize {
        self.compressed_saved.load(Ordering::Relaxed)
    }

    pub(crate) fn note_saved(&self, chars: usize) {
        if chars > 0 {
            self.compressed_saved.fetch_add(chars, Ordering::Relaxed);
        }
    }

    /// File text held for this run, if `read_file` already loaded it.
    pub(crate) fn cache_file(&self, rel: &str) -> Option<std::sync::Arc<str>> {
        self.cache.file(rel)
    }

    pub(crate) fn cache_file_put(&self, rel: String, text: std::sync::Arc<str>) {
        self.cache.put_file(rel, text);
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
                name: "list_tree".into(),
                description: "Indented directory tree under a path (depth-limited). Use for \
                    architecture / module layout pages instead of dumping every file."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "repo-relative dir, default \".\"" },
                        "depth": { "type": "integer", "description": "max depth (default 2, max 4)" },
                        "limit": { "type": "integer", "description": "max entries (default 80)" }
                    }
                }),
            },
            ToolSpec {
                name: "read_file".into(),
                description: "Read a line window of a UTF-8 text file. Prefer this over guessing: \
                    open the real source before describing behaviour. Without a range it returns \
                    the first 120 lines and tells you how many there are — pass start_line/end_line \
                    to continue. Read only the parts you will cite."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "repository-relative path, e.g. crates/atlas-store/src/lib.rs" },
                        "start_line": { "type": "integer", "description": "1-based first line (default 1)" },
                        "end_line": { "type": "integer", "description": "1-based last line (default: 120 lines from start_line)" }
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
            ToolSpec {
                name: "find_defs".into(),
                description: "Find definitions of a symbol across common languages: Rust fn/struct/\
                    enum/trait/mod, TS/JS function/class/const, Python def/class. Returns path:line \
                    plus the definition line."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "exact symbol name, e.g. Store or open_store" },
                        "glob": { "type": "string", "description": "optional glob filter" }
                    },
                    "required": ["name"]
                }),
            },
            ToolSpec {
                name: "git_log".into(),
                description: "Recent git commits (oneline). Optionally limited to one path — useful \
                    for runbooks and change history. Empty when the repo is not a git work tree."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "optional repo-relative path filter" },
                        "limit": { "type": "integer", "description": "max commits (default 15, max 50)" }
                    }
                }),
            },
        ]
    }

    /// Execute one tool call. `args` is the raw JSON arguments string produced by
    /// the model; malformed JSON is reported back as an error the model can fix.
    ///
    /// Identical calls are answered from the run-scoped snapshot cache (see
    /// [`cache`]): pages ask for the same files, and re-walking the worktree for
    /// a result another page already has is pure cost. Failures are not cached —
    /// they are what the model is expected to react to.
    pub fn call(&self, name: &str, args: &str) -> Result<String> {
        let args: Value = if args.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(args)
                .map_err(|e| anyhow!("arguments must be a JSON object: {e}"))?
        };
        // `Value` serializes with sorted keys, so the same call written with the
        // arguments in a different order still hits the same entry.
        let key = format!("{name}\u{1}{args}");
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        let out = self.run(name, &args)?;
        // The compressed form is what goes into the conversation (and into the
        // cache): read_file already applied the line-level passes itself, so it
        // is only normalized here.
        let out = match name {
            "read_file" => out,
            _ => {
                let compacted = compress::compact(&out);
                self.note_saved(compacted.removed);
                compacted.text
            }
        };
        self.cache.put(key, &out);
        Ok(out)
    }

    fn run(&self, name: &str, args: &Value) -> Result<String> {
        let glob = args
            .get("glob")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let limit = args
            .get("limit")
            .or_else(|| args.get("max_results"))
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_RESULTS as u64)
            .min(MAX_TOOL_RESULTS as u64) as usize;

        match name {
            "list_files" => self.list_files(glob.as_deref(), limit),
            "list_tree" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2)
                    .clamp(1, 4) as usize;
                self.list_tree(path, depth, limit)
            }
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
            "find_defs" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("find_defs requires a \"name\" argument"))?;
                self.find_defs(name, glob.as_deref(), limit)
            }
            "git_log" => {
                let path = args.get("path").and_then(|v| v.as_str());
                self.git_log(path, limit.clamp(1, 50))
            }
            other => Err(anyhow!("unknown tool `{other}`")),
        }
    }

    /// Resolve a repo-relative path and guarantee it cannot escape the repo.
    pub(crate) fn resolve(&self, path: &str) -> Result<PathBuf> {
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

    pub(crate) fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }

    pub(crate) fn is_redacted(&self, rel: &str) -> bool {
        self.redact.iter().any(|p| shell_glob_match(p, rel))
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
        assert!(shell_glob_match(
            "crates/**/*.rs",
            "crates/atlas-core/src/lib.rs"
        ));
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

        assert!(
            t.read_file("src/lib.rs", None, None)
                .unwrap()
                .contains("fn main")
        );
        assert!(t.read_file("../../etc/hosts", None, None).is_err());
        assert!(
            t.read_file("C:\\Windows\\System32\\drivers\\etc\\hosts", None, None)
                .is_err()
        );
        assert!(t.read_file(".env", None, None).is_err());
        assert!(
            t.call(
                "read_file",
                "{\"path\":\"src/lib.rs\",\"start_line\":1,\"end_line\":1}"
            )
            .unwrap()
            .contains("1| fn main")
        );

        // 未指定行范围时只返回一个窗口，并指出后续从哪里续读：
        // 整文件读取会被客户端截断，模型反而要多花一轮。
        let long = (1..=300)
            .map(|i| format!("let v{i} = {i};\n"))
            .collect::<String>();
        std::fs::write(dir.join("src/long.rs"), long).unwrap();
        let window = t.read_file("src/long.rs", None, None).unwrap();
        assert!(window.contains("lines 1-120 of 300"), "got {window}");
        assert!(window.contains("start_line=121"), "got {window}");
        assert!(
            !window.contains("v121 ="),
            "window leaked past its end: {window}"
        );
        let rest = t.read_file("src/long.rs", Some(121), Some(300)).unwrap();
        assert!(rest.contains("v121 = 121"), "got {rest}");

        let hits = t
            .call("grep", "{\"pattern\":\"MAIN\",\"glob\":\"**/*.rs\"}")
            .unwrap();
        assert!(hits.contains("src/lib.rs:1"), "got {hits}");
        assert!(
            t.call("grep", "{\"pattern\":\"SECRET\"}")
                .unwrap()
                .starts_with("(no matches")
        );
        assert!(t.call("read_file", "not json at all").is_err());

        let defs = t.call("find_defs", "{\"name\":\"main\"}").unwrap();
        assert!(defs.contains("src/lib.rs"), "got {defs}");
        let tree = t.call("list_tree", "{\"depth\":2}").unwrap();
        assert!(tree.contains("src/"), "got {tree}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 同一文件被多页读到时不重复读盘：本次运行内返回第一次的快照。
    #[test]
    fn identical_calls_hit_the_snapshot_cache() {
        let dir = std::env::temp_dir().join(format!("atlas-tools-cache-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("src/other.rs"), "line1\nline2\nline3\n").unwrap();
        let t = tools(&dir);

        let first = t
            .call("read_file", "{\"path\":\"src/lib.rs\",\"start_line\":1}")
            .unwrap();
        // 运行期内仓库被视为冻结：改了盘上的内容，页面之间仍看到同一份快照。
        std::fs::write(dir.join("src/lib.rs"), "fn other() {}\n").unwrap();
        let second = t
            .call("read_file", "{\"path\":\"src/lib.rs\",\"start_line\":1}")
            .unwrap();
        assert_eq!(first, second);
        assert!(second.contains("fn main"), "got {second}");

        // 参数顺序不同是同一个调用（JSON 键序规范化）。
        let reordered = t
            .call("read_file", "{\"start_line\":1,\"path\":\"src/lib.rs\"}")
            .unwrap();
        assert_eq!(reordered, first);

        // 换一个行范围：结果不同，但要靠**文件快照**命中，不重新读盘
        // （盘上那份已经被删掉了，能答出来就说明用的是快照）。
        let window = t
            .call(
                "read_file",
                "{\"path\":\"src/other.rs\",\"start_line\":2,\"end_line\":2}",
            )
            .unwrap();
        assert!(window.contains("line2"), "got {window}");
        let whole = t
            .call("read_file", "{\"path\":\"src/other.rs\",\"start_line\":1}")
            .unwrap();
        assert!(whole.contains("line3"), "got {whole}");
        assert_ne!(window, whole);
        assert_eq!(
            t.cache_stats().snapshot_hits,
            1,
            "第二个行范围应命中文件快照"
        );

        let stats = t.cache_stats();
        assert_eq!(stats.hits, 2, "{stats:?}");
        assert_eq!(stats.misses, 3, "{stats:?}");
        assert_eq!(stats.snapshots, 2, "两个文件各留一份快照");

        // 失败（路径不存在、参数非法）不进缓存：模型需要看到真实的错误。
        assert!(t.call("read_file", "{\"path\":\"missing.rs\"}").is_err());
        assert!(t.call("read_file", "{\"path\":\"missing.rs\"}").is_err());
        assert!(t.call("read_file", "not json").is_err());
        assert_eq!(t.cache_stats().misses, 5, "{:?}", t.cache_stats());
        assert_eq!(t.cache_stats().hits, 2, "{:?}", t.cache_stats());

        std::fs::remove_dir_all(&dir).ok();
    }
}
