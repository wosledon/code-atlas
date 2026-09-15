use super::*;

impl Store {
    /// Retrieval over the knowledge base: entities (graph) + chunks (wiki).
    ///
    /// Chunks are matched through the FTS5 index when available, then topped up
    /// with `LIKE` for the terms FTS cannot express (queries shorter than the
    /// trigram window, punctuation-heavy terms).
    pub fn search(&self, q: &str, limit: i64, mode: SearchMode) -> Result<Vec<SearchHit>> {
        let terms = query_terms(q);
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.conn();
        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1) graph entities — always keyword matched (no FTS index for them)
        for term in &terms {
            let like = format!("%{}%", term);
            let mut stmt = conn.prepare(
                "SELECT id, kind, name, canonical_key FROM entities
                 WHERE status='active' AND (name LIKE ?1 OR canonical_key LIKE ?1)
                 LIMIT ?2",
            )?;
            for row in stmt.query_map(params![like, limit], |r| {
                Ok(SearchHit::entity(
                    r.get(1)?,
                    r.get(0)?,
                    r.get(2)?,
                    r.get(3)?,
                ))
            })? {
                let h = row?;
                if seen.insert(format!("e:{}", h.id)) {
                    hits.push(h);
                }
            }
        }

        // 2) wiki chunks — FTS5 first
        let use_fts = mode.allows_fts() && self.fts_available();
        let (fts_terms, short_terms): (Vec<&String>, Vec<&String>) = terms
            .iter()
            .partition(|t| t.chars().count() >= 3);
        let mut chunk_hits: Vec<SearchHit> = Vec::new();
        if use_fts && !fts_terms.is_empty() {
            match Self::search_fts(&conn, &fts_terms, limit) {
                Ok(h) => chunk_hits = h,
                Err(e) => {
                    tracing::warn!("fts5 query failed, falling back to LIKE: {e:#}");
                }
            }
        }
        if chunk_hits.is_empty() || !short_terms.is_empty() {
            let extra_terms: Vec<&String> = if chunk_hits.is_empty() {
                terms.iter().collect()
            } else {
                short_terms.clone()
            };
            let mut extra = Self::search_like_chunks(&conn, &extra_terms, limit)?;
            let known: std::collections::HashSet<String> =
                chunk_hits.iter().map(|h| h.id.clone()).collect();
            extra.retain(|h| !known.contains(&h.id));
            chunk_hits.extend(extra);
        }
        for h in chunk_hits {
            if seen.insert(format!("c:{}", h.id)) {
                hits.push(h);
            }
        }

        hits.sort_by(|a, b| {
            // Prefer wiki chunks for multi-term natural questions
            let rank = |h: &SearchHit| {
                let mut s = h.score;
                if h.kind == "chunk" {
                    s += 0.35;
                }
                if h.kind == "symbol" && terms.len() > 1 {
                    s -= 0.15;
                }
                s
            };
            rank(b).partial_cmp(&rank(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit as usize);
        Ok(hits)
    }

    fn search_fts(conn: &Connection, terms: &[&String], limit: i64) -> Result<Vec<SearchHit>> {
        let matcher = fts_query(terms);
        if matcher.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = conn.prepare(
            "SELECT c.id, c.page_path, c.title, c.summary, c.body, c.start_line, c.end_line,
                    bm25(chunks_fts) AS rank
             FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.chunk_id
             WHERE chunks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![matcher, limit], |r| {
                let rank: f64 = r.get(7)?;
                // bm25() is negative and lower-is-better; squash into [0.8, 1.6]
                let strength = (-rank).max(0.0);
                Ok(SearchHit {
                    kind: "chunk".into(),
                    id: r.get(0)?,
                    page_path: Some(r.get(1)?),
                    title: r.get(2)?,
                    summary: r.get(3)?,
                    score: 0.8 + 0.8 * (strength / (strength + 1.0)),
                    body: Some(r.get(4)?),
                    start_line: Some(r.get(5)?),
                    end_line: Some(r.get(6)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn search_like_chunks(
        conn: &Connection,
        terms: &[&String],
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for term in terms {
            let like = format!("%{}%", term);
            let mut stmt = conn.prepare(
                "SELECT id, page_path, title, summary, body, start_line, end_line FROM chunks
                 WHERE title LIKE ?1 OR summary LIKE ?1 OR body LIKE ?1
                 LIMIT ?2",
            )?;
            for row in stmt.query_map(params![like, limit], |r| {
                Ok(SearchHit {
                    kind: "chunk".into(),
                    id: r.get(0)?,
                    page_path: Some(r.get(1)?),
                    title: r.get(2)?,
                    summary: r.get(3)?,
                    score: 0.75,
                    body: Some(r.get(4)?),
                    start_line: Some(r.get(5)?),
                    end_line: Some(r.get(6)?),
                })
            })? {
                let mut h = row?;
                // Chunks covering more query terms (whole chunk, not just its
                // heading) rank above chunks that merely share one bigram.
                let head = format!("{} {}", h.title, h.summary).to_lowercase();
                let body = h.body.clone().unwrap_or_default().to_lowercase();
                let mut boost: f64 = 0.0;
                for t in terms {
                    let t = t.to_lowercase();
                    if t.is_empty() {
                        continue;
                    }
                    if head.contains(&t) {
                        boost += 0.1;
                    } else if body.contains(&t) {
                        boost += 0.05;
                    }
                }
                h.score += boost.min(0.4);
                if seen.insert(h.id.clone()) {
                    out.push(h);
                }
            }
        }
        Ok(out)
    }
}
