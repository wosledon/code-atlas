use super::run::{RunContext, RunCounts};
use super::write::chunk_source_label;
use super::*;

pub(crate) fn read_last_update(atlas_root: &Path) -> Option<LastUpdate> {
    let p = atlas_root.join(".last-update.json");
    let text = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn write_last_update(atlas_root: &Path, last: &LastUpdate) -> Result<()> {
    std::fs::create_dir_all(atlas_root)?;
    std::fs::write(
        atlas_root.join(".last-update.json"),
        serde_json::to_string_pretty(last)?,
    )?;
    Ok(())
}

pub(crate) fn write_index(atlas_root: &Path, plan: &[PlannedPage], cfg: &AtlasConfig) -> Result<()> {
    let mut body = String::from("# Code Atlas 文档树\n\n");
    body.push_str(&format!("语言：{}\n\n", cfg.output.language));
    if let Some(entry) = plan.iter().find(|p| p.rel_path == "quickstart.md") {
        body.push_str(&format!(
            "**入口：[{}]({}) — {}**\n\n",
            entry.title, entry.rel_path, entry.description
        ));
    }
    // group by top-level directory
    let mut groups: std::collections::BTreeMap<String, Vec<&PlannedPage>> = Default::default();
    for p in plan {
        let group = p
            .rel_path
            .split('/')
            .next()
            .unwrap_or(".")
            .to_string();
        groups.entry(group).or_default().push(p);
    }
    for (g, list) in groups {
        body.push_str(&format!("## {}\n\n", g));
        for p in list {
            body.push_str(&format!("- [{}]({}) — {}\n", p.title, p.rel_path, p.description));
        }
        body.push('\n');
    }
    let raw = format!(
        "---\nokf_version: \"0.2\"\ntitle: Code Atlas 文档树\ndescription: 文档导航\ntags: [nav]\ntype: Index\n---\n{body}"
    );
    markdown::atomic_write(&atlas_root.join("README.md"), raw.as_bytes())?;
    // keep legacy index.md pointer
    let idx = "---\nokf_version: \"0.2\"\ntitle: Index\ndescription: 入口为 quickstart.md\n---\n\n\
               从这里开始：[快速上手](./quickstart.md)。\n\n完整目录见 [README.md](./README.md)。\n";
    markdown::atomic_write(&atlas_root.join("index.md"), idx.as_bytes())?;
    Ok(())
}

pub fn reindex(repo_root: &Path, cfg: &AtlasConfig) -> Result<usize> {
    let db = cfg.db_path(repo_root);
    let atlas_root = cfg.atlas_root(repo_root);
    let store = Store::open(&db)?;
    let run_id = Uuid::new_v4().to_string();
    store.begin_run(&run_id, "reindex", None, None, Some(&cfg.output.language))?;
    let mut n = 0;
    if atlas_root.exists() {
        for entry in walkdir::WalkDir::new(&atlas_root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&atlas_root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel == "index.md" {
                continue;
            }
            let text = std::fs::read_to_string(entry.path())?;
            let hash = body_hash(&text);
            // Reindexing must not downgrade what `update` already learned about a
            // page: keep its title/type/description and evidence fingerprint so
            // `atlas update` still knows whether the page needs regenerating and
            // the UI keeps real page types (highlights are picked by type).
            let prev = store.get_page(&rel)?;
            let (title, page_type, description, evidence) = match prev {
                Some(p) => (p.title, p.page_type, p.description, p.evidence_hash),
                // Unknown page (DB rebuilt from scratch): fall back to the H1 or
                // the path, and to the generic type.
                None => (
                    first_heading(&text).unwrap_or_else(|| rel.clone()),
                    "Reference".to_string(),
                    String::new(),
                    String::new(),
                ),
            };
            store.upsert_page(
                &rel,
                &title,
                &page_type,
                &description,
                &hash,
                &evidence,
                &run_id,
            )?;
            let specs = atlas_kb::merge_small_chunks(atlas_kb::structural_chunks(
                &text,
                cfg.kb.chunk.target_tokens,
            ));
            // 重建索引时只做结构分块，但源标签与 `update` 保持一致，避免下一次
            // `atlas update` 因为标签不匹配而重做全部 chunks。
            let label = chunk_source_label(&cfg.kb.chunk.mode, cfg.kb.chunk.target_tokens, false);
            atlas_kb::store_chunks(
                &store,
                &rel,
                &run_id,
                &specs,
                &format!("structural|{label}"),
            )?;
            n += 1;
        }
    }
    store.finish_run(&run_id, "succeeded", None, 0, 0, 0.0, None)?;
    Ok(n)
}

/// First level-1 heading of a markdown document, used as a title fallback when
/// the index has no metadata for a page (e.g. a database rebuilt from markdown).
pub(crate) fn first_heading(text: &str) -> Option<String> {
    let mut in_fence = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(title) = line.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return Some(title.chars().take(120).collect());
            }
        }
    }
    None
}

pub fn open_store(repo_root: &Path, cfg: &AtlasConfig) -> Result<Store> {
    let p = cfg.db_path(repo_root);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let store = Store::open(&p)?;
    // Runs whose process died leave a `running` row behind; reconcile them.
    let _ = store.fail_stale_runs(0);
    Ok(store)
}

/// `atlas check`: the wiki must be navigable (entry page + document tree) and
/// every page must resolve its relative links.
pub fn run_check(repo_root: &Path, cfg: &AtlasConfig) -> Result<Vec<String>> {
    let atlas_root = cfg.atlas_root(repo_root);
    let mut problems = Vec::new();
    for required in ["quickstart.md", "README.md", "index.md"] {
        if !atlas_root.join(required).exists() {
            problems.push(format!("missing {required} under {}", atlas_root.display()));
        }
    }
    if atlas_root.join("quickstart.md").exists() {
        let text = std::fs::read_to_string(atlas_root.join("quickstart.md"))?;
        if text.trim().chars().count() < 120 {
            problems.push("quickstart.md looks empty (template fallback?)".into());
        }
    }
    problems.extend(markdown::link_check(&atlas_root));
    if problems.iter().any(|p| p.starts_with("missing ")) {
        bail!("{}", problems.join("; "));
    }
    Ok(problems)
}

/// Close a run: drop indexes of pages that no longer exist, refresh the doc
/// index, record the update metadata and assemble the run result.
pub(super) fn finalize(run: &RunContext<'_>, counts: RunCounts) -> Result<RunResult> {
    let ctx = run.ctx;
    let session = run.session;
    let store = &session.store;
    let run_id = session.run_id.as_str();
    let atlas_root = &session.atlas_root;
    let mut notes = counts.notes;

    // Pages left over from an older doc layout would otherwise stay indexed
    // forever and pollute search with paths that no longer exist.
    let pruned = store.prune_missing_pages(atlas_root)?;
    if pruned > 0 {
        notes.push(format!(
            "pruned {pruned} obsolete pages (chunks included) that are no longer in the doc tree"
        ));
        println!("[atlas] 清理 {pruned} 个已失效页面索引（含其分块）");
    }
    let total_chunks = store.count_chunks()? as usize;

    // index.md
    println!("[atlas] 写文档索引 README.md / index.md …");
    write_index(atlas_root, run.plan, &ctx.cfg)?;

    let broken = markdown::link_check(atlas_root);
    if !broken.is_empty() {
        // prefixed so `--ci` can distinguish hard problems from advisory notes
        notes.push(format!("problem: {} broken relative links", broken.len()));
        println!("[atlas] 发现 {} 条相对链接问题", broken.len());
    }

    let last = LastUpdate {
        updatedAt: chrono::Utc::now().to_rfc3339(),
        command: session.mode.clone(),
        gitHead: session.git_head.clone(),
        model: Some(ctx.cfg.llm.model.clone()),
        runId: run_id.to_string(),
        language: ctx.cfg.output.language.clone(),
    };
    write_last_update(atlas_root, &last)?;

    // agents pointer for in-repo
    if ctx.cfg.output.strategy == "in-repo" {
        let rel = ctx.cfg.output.atlas_root.clone();
        ensure_agents_pointer(&ctx.repo_root, &rel)?;
    }

    let entities = store.list_entities(None, 10_000)?.len();
    Ok(RunResult {
        run_id: run_id.to_string(),
        mode: session.mode.clone(),
        status: "succeeded".into(),
        pages_written: counts.pages_written,
        entities,
        chunks: total_chunks,
        no_op: false,
        prompt_tokens: counts.prompt_tokens,
        completion_tokens: counts.completion_tokens,
        notes,
    })
}
