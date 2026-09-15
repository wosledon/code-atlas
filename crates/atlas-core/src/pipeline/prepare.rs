use super::evidence::{build_evidence, page_fingerprint, read_page_body};
use super::run::RunContext;
use super::types::PageJob;
use super::*;

/// Fingerprint every planned page and reuse the previous body whenever the
/// evidence it was written from has not changed: those pages skip the LLM.
pub(super) fn prepare_jobs(run: &RunContext<'_>) -> Result<Vec<PageJob>> {
    let store = &run.session.store;
    let scan: &RepoScan = run.scan;
    let use_llm = run.use_llm;

    let jobs: Vec<PageJob> = run
        .plan
        .iter()
        .map(|page| {
            let evidence = build_evidence(scan, page);
            let fingerprint = page_fingerprint(scan, page, &evidence);
            let reuse_body = if use_llm {
                match store.get_page(&page.rel_path)? {
                    Some(row) if row.evidence_hash == fingerprint => {
                        read_page_body(&run.session.atlas_root.join(&page.rel_path))
                    }
                    _ => None,
                }
            } else {
                None
            };
            Ok(PageJob {
                page: page.clone(),
                evidence,
                fingerprint,
                reuse_body,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let reuse_planned = jobs.iter().filter(|j| j.reuse_body.is_some()).count();
    if reuse_planned > 0 {
        println!("[atlas] {reuse_planned} 页证据未变，复用上一版正文（跳过 LLM）");
    }
    Ok(jobs)
}
