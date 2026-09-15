//! SQLite-backed store for pages, chunks, entities, runs and search.
//!
//! The [`Store`] type lives here together with the connection/opening logic;
//! the query surface is split by responsibility into the modules below, each
//! adding an `impl Store` block.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub mod schema;

mod chunks;
mod entities;
mod models;
mod pages;
mod query;
mod runs;
mod search;

#[cfg(test)]
mod tests;

pub use models::*;
pub use query::*;

pub struct Store {
    conn: Mutex<Connection>,
    path: PathBuf,
    fts: std::sync::atomic::AtomicBool,
}

impl Store {
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db dir {}", parent.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("open sqlite {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
            fts: std::sync::atomic::AtomicBool::new(false),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
            fts: std::sync::atomic::AtomicBool::new(false),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the SQLite build provides a working FTS5 index for chunks.
    pub fn fts_available(&self) -> bool {
        self.fts.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch(schema::MIGRATION_V1)?;
        // ADD COLUMN migrations are tolerated when already applied.
        let _ = conn.execute_batch(schema::MIGRATION_PAGE_EVIDENCE_HASH);
        let fts_ok = conn
            .execute_batch(schema::MIGRATION_FTS_TRIGRAM)
            .or_else(|_| conn.execute_batch(schema::MIGRATION_FTS_UNICODE61))
            .is_ok();
        drop(conn);
        self.fts
            .store(fts_ok, std::sync::atomic::Ordering::Relaxed);
        if fts_ok {
            self.backfill_fts()?;
        }
        Ok(())
    }

    /// Rebuild the full-text index from `chunks` when it is empty (fresh DB
    /// upgrade) or out of sync.
    pub fn backfill_fts(&self) -> Result<usize> {
        if !self.fts_available() {
            return Ok(0);
        }
        let conn = self.conn();
        let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM chunks_fts", [], |r| r.get(0))?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        if indexed == total {
            return Ok(0);
        }
        conn.execute("DELETE FROM chunks_fts", [])?;
        let inserted = conn.execute(
            "INSERT INTO chunks_fts (chunk_id, page_path, title, summary, body)
             SELECT id, page_path, title, summary, body FROM chunks",
            [],
        )?;
        Ok(inserted)
    }
}
