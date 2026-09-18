use super::evidence::{
    build_evidence_shared, build_evidence_with_shared, page_fingerprint, read_page_body,
};
use super::prompt::prompt_salt;
use super::quality::is_polluted;
use super::run::RunContext;
use super::types::PageJob;
use super::*;

/// Fingerprint every planned page and reuse the previous body whenever the
/// repository evidence *and* the generation inputs (prompts, model, knobs) are
/// unchanged: those pages skip the LLM. Polluted bodies (tool transcripts, draft
/// markers) are never reused — they force regeneration even when the hash matches.
pub(super) fn prepare_jobs(run: &RunContext<'_>) -> Result<Vec<PageJob>> {
    let store = &run.session.store;
    let scan: &RepoScan = run.scan;
    let use_llm = run.use_llm;
    let cfg = &run.ctx.cfg;

    // Shared evidence fragments (repo header, manifests, wiki index) once per run.
    let shared = build_evidence_shared(scan, run.plan);

    let jobs: Vec<PageJob> = run
        .plan
        .iter()
        .map(|page| {
            let evidence = build_evidence_with_shared(scan, page, &shared);
            let salt = prompt_salt(cfg, page, run.llm.provider(), run.llm.model());
            let fingerprint = page_fingerprint(scan, page, &evidence, &salt);
            let row = store.get_page(&page.rel_path)?;
            let prev_is_template = row
                .as_ref()
                .is_some_and(|r| r.evidence_hash.ends_with("+template"));
            // Read the on-disk body at most once per page.
            let disk_body = read_page_body(&run.session.atlas_root.join(&page.rel_path));
            let reuse_body = if use_llm {
                match &row {
                    Some(r) if r.evidence_hash == fingerprint => disk_body.clone().filter(|b| {
                        if is_polluted(b) {
                            println!(
                                "[atlas] {} 含工具调用残片，忽略复用，强制重生成",
                                page.rel_path
                            );
                            false
                        } else {
                            true
                        }
                    }),
                    _ => None,
                }
            } else {
                None
            };
            // Only a page we could actually keep is worth protecting: templates
            // are cheap to rebuild, real (clean) bodies are not.
            let keep_body_on_failure =
                !prev_is_template && disk_body.as_ref().is_some_and(|b| !is_polluted(b));
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
