use super::*;

impl Store {
    pub fn count_entities(&self) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM entities WHERE status='active'",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn upsert_entity(
        &self,
        kind: &str,
        name: &str,
        canonical_key: &str,
        props: &serde_json::Value,
        status: &str,
        run_id: &str,
    ) -> Result<String> {
        let conn = self.conn();
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM entities WHERE canonical_key=?1",
                params![canonical_key],
                |r| r.get(0),
            )
            .optional()?;
        let id = existing.unwrap_or_else(|| {
            format!(
                "ent_{}",
                canonical_key
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>()
            )
        });
        conn.execute(
            "INSERT INTO entities (id, kind, name, canonical_key, props, status, updated_run)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(canonical_key) DO UPDATE SET
               kind=excluded.kind, name=excluded.name, props=excluded.props,
               status=excluded.status, updated_run=excluded.updated_run",
            params![id, kind, name, canonical_key, props.to_string(), status, run_id],
        )?;
        Ok(id)
    }

    pub fn upsert_relation(
        &self,
        src_id: &str,
        dst_id: &str,
        rel: &str,
        props: &serde_json::Value,
        confidence: f64,
        run_id: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO relations (src_id, dst_id, rel, props, confidence, run_id)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(src_id, dst_id, rel) DO UPDATE SET
               props=excluded.props, confidence=excluded.confidence, run_id=excluded.run_id",
            params![src_id, dst_id, rel, props.to_string(), confidence, run_id],
        )?;
        Ok(())
    }

    fn map_entity(r: &rusqlite::Row) -> rusqlite::Result<EntityRow> {
        Ok(EntityRow {
            id: r.get(0)?,
            kind: r.get(1)?,
            name: r.get(2)?,
            canonical_key: r.get(3)?,
            props: r.get(4)?,
            status: r.get(5)?,
        })
    }

    pub fn list_entities(&self, kind: Option<&str>, limit: i64) -> Result<Vec<EntityRow>> {
        let conn = self.conn();
        let mut out = Vec::new();
        if let Some(k) = kind {
            let mut stmt = conn.prepare(
                "SELECT id, kind, name, canonical_key, props, status
                 FROM entities WHERE status='active' AND kind=?1
                 ORDER BY name LIMIT ?2",
            )?;
            for row in stmt.query_map(params![k, limit], Self::map_entity)? {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, kind, name, canonical_key, props, status
                 FROM entities WHERE status='active'
                 ORDER BY name LIMIT ?1",
            )?;
            for row in stmt.query_map(params![limit], Self::map_entity)? {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn list_relations(&self, limit: i64) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT src_id, dst_id, rel FROM relations LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn neighborhood(
        &self,
        entity_id: &str,
        limit: i64,
    ) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT src_id, dst_id, rel FROM relations
             WHERE src_id=?1 OR dst_id=?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![entity_id, limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_claim(
        &self,
        page_path: &str,
        statement: &str,
        evidence: &str,
        status: &str,
        run_id: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO claims (page_path, statement, evidence, status, run_id)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(page_path, statement) DO UPDATE SET
               evidence=excluded.evidence, status=excluded.status, run_id=excluded.run_id",
            params![page_path, statement, evidence, status, run_id],
        )?;
        Ok(())
    }
}
