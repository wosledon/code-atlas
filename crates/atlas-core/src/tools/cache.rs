//! Snapshot cache for tool output, scoped to one run.
//!
//! Every page task shares the same [`RepoTools`](super::RepoTools), and pages
//! read the same handful of files (manifests, entry points, schema, lib.rs), so
//! identical `read_file` / `grep` / `list_tree` calls repeat many times over a
//! run. The repository is frozen for the duration of a run (the run holds the
//! lock and fingerprints the worktree), which is what makes serving an earlier
//! read safe: a page cannot see a file change mid-run that a later `update`
//! would not then rewrite anyway.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Ceiling for everything kept. Hits are the common case (the same files for
/// every page), so the cache is worth having, but a monorepo must not pin its
/// whole source tree in memory: past this, results are served without being
/// remembered.
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Results larger than this are not remembered: output is already bounded per
/// tool, so anything this big is a whole-file dump that would crowd out the
/// small files every page reads.
const MAX_ENTRY_BYTES: usize = 512 * 1024;

/// A single file snapshot bigger than this is read normally but not held.
const MAX_FILE_SNAPSHOT: usize = 1024 * 1024;

/// Total file snapshots held per run.
const MAX_FILES_BYTES: usize = 16 * 1024 * 1024;

/// What the cache did over a run, for the run summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub bytes: usize,
    /// Files whose text is held for the rest of the run.
    pub snapshots: usize,
    /// `read_file` calls answered from a snapshot of a file read earlier —
    /// including window shifts and overlaps the per-call cache cannot serve.
    pub snapshot_hits: usize,
}

impl CacheStats {
    pub fn lookups(&self) -> usize {
        self.hits + self.misses
    }

    /// Share of lookups answered without touching the repository.
    pub fn hit_rate(&self) -> f64 {
        if self.lookups() == 0 {
            0.0
        } else {
            self.hits as f64 / self.lookups() as f64
        }
    }

    /// Bytes held, in a unit that stays readable for a small repository (a
    /// handful of files rounds to `0 KiB` otherwise).
    pub fn held(&self) -> String {
        const KIB: usize = 1024;
        if self.bytes < KIB {
            format!("{} B", self.bytes)
        } else if self.bytes < KIB * KIB {
            format!("{:.1} KiB", self.bytes as f64 / KIB as f64)
        } else {
            format!("{:.1} MiB", self.bytes as f64 / (KIB * KIB) as f64)
        }
    }
}

#[derive(Default)]
pub(crate) struct ToolCache {
    entries: Mutex<HashMap<String, String>>,
    bytes: AtomicUsize,
    hits: AtomicUsize,
    misses: AtomicUsize,
    /// Whole-file text, keyed by repo-relative path. A page that reads lines
    /// 1–120 and later 100–200 of the same file must not touch the disk twice:
    /// the per-call cache cannot serve the shifted window, a file snapshot can.
    files: Mutex<HashMap<String, Arc<str>>>,
    file_bytes: AtomicUsize,
    snapshot_hits: AtomicUsize,
}

impl ToolCache {
    /// The stored result for `key`, counting the lookup either way. A poisoned
    /// lock or a missing entry both mean "run the tool".
    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let hit = self
            .entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).cloned());
        if hit.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }

    pub(crate) fn put(&self, key: String, value: &str) {
        if value.len() > MAX_ENTRY_BYTES {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        // Racing writers can push the total a little past the cap; the budget is
        // there to bound growth, not to be exact.
        if self.bytes.load(Ordering::Relaxed) + value.len() > MAX_BYTES {
            return;
        }
        match entries.insert(key, value.to_string()) {
            Some(prev) => {
                self.bytes.fetch_sub(prev.len(), Ordering::Relaxed);
                self.bytes.fetch_add(value.len(), Ordering::Relaxed);
            }
            None => {
                self.bytes.fetch_add(value.len(), Ordering::Relaxed);
            }
        }
    }

    /// The file text if this run has already read it, counting a snapshot hit.
    pub(crate) fn file(&self, rel: &str) -> Option<Arc<str>> {
        let hit = self
            .files
            .lock()
            .ok()
            .and_then(|files| files.get(rel).cloned());
        if hit.is_some() {
            self.snapshot_hits.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }

    /// Remember a file's text for the rest of the run. Oversized files are read
    /// normally but not held, so a monorepo cannot pin its whole tree in memory.
    pub(crate) fn put_file(&self, rel: String, text: Arc<str>) {
        if text.len() > MAX_FILE_SNAPSHOT {
            return;
        }
        let Ok(mut files) = self.files.lock() else {
            return;
        };
        if self.file_bytes.load(Ordering::Relaxed) + text.len() > MAX_FILES_BYTES {
            return;
        }
        if files.insert(rel, text.clone()).is_none() {
            self.file_bytes.fetch_add(text.len(), Ordering::Relaxed);
        }
    }

    pub(crate) fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            snapshots: self.files.lock().map(|f| f.len()).unwrap_or_default(),
            snapshot_hits: self.snapshot_hits.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_a_stored_result_and_counts_the_lookup() {
        let cache = ToolCache::default();
        assert!(cache.get("read_file|src/lib.rs").is_none());
        cache.put("read_file|src/lib.rs".into(), "fn main() {}");
        assert_eq!(
            cache.get("read_file|src/lib.rs").as_deref(),
            Some("fn main() {}")
        );
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate(), 0.5);
        assert_eq!(stats.bytes, "fn main() {}".len());
        assert_eq!(stats.held(), "12 B");
    }

    /// 小仓库不该显示成 `0 KiB`。
    #[test]
    fn held_uses_a_readable_unit() {
        let kb = CacheStats {
            bytes: 2 * 1024 + 512,
            ..Default::default()
        };
        assert_eq!(kb.held(), "2.5 KiB");
        let mb = CacheStats {
            bytes: 3 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(mb.held(), "3.0 MiB");
    }

    /// 文件快照：同一文件只读一次盘，之后任意行范围都命中。
    #[test]
    fn file_snapshots_serve_any_later_range() {
        let cache = ToolCache::default();
        assert!(cache.file("src/lib.rs").is_none());
        cache.put_file("src/lib.rs".into(), Arc::from("line1\nline2\nline3\n"));
        assert!(cache.file("src/lib.rs").is_some());
        assert!(cache.file("src/lib.rs").is_some());
        let stats = cache.stats();
        assert_eq!(stats.snapshots, 1);
        assert_eq!(stats.snapshot_hits, 2);
    }

    #[test]
    fn keeps_distinct_keys_apart() {
        let cache = ToolCache::default();
        cache.put("read_file|a.rs".into(), "a");
        cache.put("read_file|b.rs".into(), "b");
        assert_eq!(cache.get("read_file|a.rs").as_deref(), Some("a"));
        assert_eq!(cache.get("read_file|b.rs").as_deref(), Some("b"));
        assert_eq!(cache.stats().bytes, 2);
    }

    #[test]
    fn oversized_results_are_served_but_not_kept() {
        let cache = ToolCache::default();
        let huge = "x".repeat(MAX_ENTRY_BYTES + 1);
        cache.put("big".into(), &huge);
        assert!(cache.get("big").is_none());
        assert_eq!(cache.stats().bytes, 0);
    }

    #[test]
    fn rewriting_a_key_does_not_double_count_bytes() {
        let cache = ToolCache::default();
        cache.put("k".into(), "first");
        cache.put("k".into(), "second-longer");
        assert_eq!(cache.get("k").as_deref(), Some("second-longer"));
        assert_eq!(cache.stats().bytes, "second-longer".len());
    }
}
