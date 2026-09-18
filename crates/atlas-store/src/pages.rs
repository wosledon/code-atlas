use super::*;

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_page(
        &self,
        path: &str,
        title: &str,
        page_type: &str,
        description: &str,
        body_hash: &str,
        evidence_hash: &str,
        run_id: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO pages (path, title, type, description, body_hash, evidence_hash, run_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(path) DO UPDATE SET
               title=excluded.title, type=excluded.type,
               description=excluded.description, body_hash=excluded.body_hash,
               evidence_hash=excluded.evidence_hash,
               run_id=excluded.run_id",
            params![path, title, page_type, description, body_hash, evidence_hash, run_id],
        )?;
        Ok(())
    }

    pub fn get_page(&self, path: &str) -> Result<Option<PageRow>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT path, title, type, description, body_hash, evidence_hash
                 FROM pages WHERE path=?1",
                params![path],
                |r| {
                    Ok(PageRow {
                        path: r.get(0)?,
                        title: r.get(1)?,
                        page_type: r.get(2)?,
                        description: r.get(3)?,
                        body_hash: r.get(4)?,
                        evidence_hash: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn count_pages(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM pages", [], |r| r.get(0))?)
    }

    /// All indexed pages in insertion order (plan order), for navigation surfaces
    /// such as the landing page highlights.
    pub fn list_pages(&self) -> Result<Vec<PageRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT path, title, type, description, body_hash, evidence_hash FROM pages ORDER BY rowid",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PageRow {
                    path: r.get(0)?,
                    title: r.get(1)?,
                    page_type: r.get(2)?,
                    description: r.get(3)?,
                    body_hash: r.get(4)?,
                    evidence_hash: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Drop page and chunk rows whose markdown file no longer exists under
    /// `atlas_root` (page layout changed between runs). Returns how many pages
    /// were removed; their chunks (and FTS rows) go with them.
    pub fn prune_missing_pages(&self, atlas_root: &Path) -> Result<usize> {
        let conn = self.conn();
        let stale: Vec<String> = conn
            .prepare("SELECT path FROM pages")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| !atlas_root.join(p).is_file())
            .collect();
        if stale.is_empty() {
            return Ok(0);
        }
        for path in &stale {
            conn.execute(
                "DELETE FROM chunk_entities WHERE chunk_id IN (SELECT id FROM chunks WHERE page_path=?1)",
                params![path],
            )?;
            conn.execute("DELETE FROM chunks WHERE page_path=?1", params![path])?;
            if self.fts_available() {
                conn.execute("DELETE FROM chunks_fts WHERE page_path=?1", params![path])?;
            }
            conn.execute("DELETE FROM pages WHERE path=?1", params![path])?;
        }
        Ok(stale.len())
    }
}
