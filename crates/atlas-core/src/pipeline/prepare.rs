use super::evidence::{build_evidence, page_fingerprint, read_page_body};
use super::prompt::prompt_salt;
use super::run::RunContext;
use super::types::PageJob;
use super::*;

/// Fingerprint every planned page and reuse the previous body whenever the
/// repository evidence *and* the generation inputs (prompts, model, knobs) are
/// unchanged: those pages skip the LLM.
pub(super) fn prepare_jobs(run: &RunContext<'_>) -> Result<Vec<PageJob>> {
    let store = &run.session.store;
    let scan: &RepoScan = run.scan;
    let use_llm = run.use_llm;
    let cfg = &run.ctx.cfg;

    let jobs: Vec<PageJob> = run
        .plan
        .iter()
        .map(|page| {
            let evidence = build_evidence(scan, page);
            let salt = prompt_salt(cfg, page, run.llm.provider(), run.llm.model());
            let fingerprint = page_fingerprint(scan, page, &evidence, &salt);
            let row = store.get_page(&page.rel_path)?;
            let prev_is_template = row
                .as_ref()
                .is_some_and(|r| r.evidence_hash.ends_with("+template"));
            let body_on_disk =
                || read_page_body(&run.session.atlas_root.join(&page.rel_path));
            let reuse_body = if use_llm {
                match &row {
                    Some(r) if r.evidence_hash == fingerprint => body_on_disk(),
                    _ => None,
                }
            } else {
                None
            };
            // Only a page we could actually keep is worth protecting: templates
            // are cheap to rebuild, real bodies are not.
            let keep_body_on_failure = !prev_is_template && body_on_disk().is_some();
            Ok(PageJob {
                page: page.clone(),
                evidence,
                fingerprint,
                reuse_body,
                keep_body_on_failure,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let reuse_planned = jobs.iter().filter(|j| j.reuse_body.is_some()).count();
    if reuse_planned > 0 {
        println!("[atlas] {reuse_planned} 页证据未变，复用上一版正文（跳过 LLM）");
    }
    Ok(jobs)
}
