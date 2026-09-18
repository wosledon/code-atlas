use super::*;

impl Store {
    /// Mark runs left in `running` by an interrupted process as failed.
    pub fn fail_stale_runs(&self, older_than_secs: i64) -> Result<usize> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(older_than_secs)).to_rfc3339();
        let now = chrono::Utc::now().to_rfc3339();
        let n = self.conn().execute(
            "UPDATE runs SET status='failed',
                    error=COALESCE(error, 'interrupted: no completion record'),
                    finished_at=?1
             WHERE status='running' AND created_at < ?2",
            params![now, cutoff],
        )?;
        Ok(n)
    }

    pub fn begin_run(
        &self,
        id: &str,
        mode: &str,
        provider: Option<&str>,
        model: Option<&str>,
        language: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO runs (id, mode, status, provider, model, language, created_at)
             VALUES (?1, ?2, 'running', ?3, ?4, ?5, ?6)",
            params![id, mode, provider, model, language, now],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_run(
        &self,
        id: &str,
        status: &str,
        git_head: Option<&str>,
        prompt_tokens: i64,
        completion_tokens: i64,
        est_cost_usd: f64,
        error: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE runs SET status=?2, git_head=COALESCE(?3, git_head),
             prompt_tokens=?4, completion_tokens=?5, est_cost_usd=?6,
             error=?7, finished_at=?8 WHERE id=?1",
            params![id, status, git_head, prompt_tokens, completion_tokens, est_cost_usd, error, now],
        )?;
        Ok(())
    }

    fn map_run(r: &rusqlite::Row) -> rusqlite::Result<RunRecord> {
        Ok(RunRecord {
            id: r.get(0)?,
            mode: r.get(1)?,
            status: r.get(2)?,
            git_head: r.get(3)?,
            provider: r.get(4)?,
            model: r.get(5)?,
            language: r.get(6)?,
            prompt_tokens: r.get(7)?,
            completion_tokens: r.get(8)?,
            est_cost_usd: r.get(9)?,
            error: r.get(10)?,
            created_at: r.get(11)?,
            finished_at: r.get(12)?,
        })
    }

    pub fn get_run(&self, id: &str) -> Result<Option<RunRecord>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT id, mode, status, git_head, provider, model, language,
                        prompt_tokens, completion_tokens, est_cost_usd, error, created_at, finished_at
                 FROM runs WHERE id=?1",
                params![id],
                Self::map_run,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_runs(&self, limit: i64) -> Result<Vec<RunRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, mode, status, git_head, provider, model, language,
                    prompt_tokens, completion_tokens, est_cost_usd, error, created_at, finished_at
             FROM runs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], Self::map_run)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_llm_call(
        &self,
        run_id: &str,
        stage: &str,
        provider: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        latency_ms: i64,
        status: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO llm_calls
             (run_id, stage, provider, model, prompt_tokens, completion_tokens, latency_ms, status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                run_id,
                stage,
                provider,
                model,
                prompt_tokens,
                completion_tokens,
                latency_ms,
                status,
                now
            ],
        )?;
        Ok(())
    }

    pub fn count_runs(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))?)
    }
}
