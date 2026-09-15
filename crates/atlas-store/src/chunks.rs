use super::*;

impl Store {
    pub fn count_chunks(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?)
    }

    pub fn count_chunks_for_page(&self, page_path: &str) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM chunks WHERE page_path=?1",
            params![page_path],
            |r| r.get(0),
        )?)
    }

    /// How many of a page's chunks carry a chunker signature `suffix` (the part
    /// after the last `|`). Chunk reuse is only valid when *all* of them match the
    /// current chunker (see `pipeline::write`), so switching `kb.chunk.mode` or
    /// `target_tokens` rebuilds them.
    pub fn count_chunks_for_page_with_signature(
        &self,
        page_path: &str,
        suffix: &str,
    ) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM chunks WHERE page_path=?1 AND source LIKE ?2",
            params![page_path, format!("%|{suffix}")],
            |r| r.get(0),
        )?)
    }

    pub fn replace_chunks_for_page(
        &self,
        page_path: &str,
        run_id: &str,
        chunks: &[ChunkRow],
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM chunk_entities WHERE chunk_id IN (SELECT id FROM chunks WHERE page_path=?1)",
            params![page_path],
        )?;
        conn.execute("DELETE FROM chunks WHERE page_path=?1", params![page_path])?;
        if self.fts_available() {
            conn.execute("DELETE FROM chunks_fts WHERE page_path=?1", params![page_path])?;
        }
        for c in chunks {
            conn.execute(
                "INSERT INTO chunks
                 (id, page_path, ord, title, summary, body, start_line, end_line, source, body_hash, run_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    c.id,
                    c.page_path,
                    c.ord,
                    c.title,
                    c.summary,
                    c.body,
                    c.start_line,
                    c.end_line,
                    c.source,
                    body_hash(&c.body),
                    run_id
                ],
            )?;
            if self.fts_available() {
                conn.execute(
                    "INSERT INTO chunks_fts (chunk_id, page_path, title, summary, body)
                     VALUES (?1,?2,?3,?4,?5)",
                    params![c.id, c.page_path, c.title, c.summary, c.body],
                )?;
            }
        }
        Ok(())
    }
}
