use super::*;

fn chunk(id: &str, page: &str, ord: i64, title: &str, summary: &str, body: &str) -> ChunkRow {
    ChunkRow {
        id: id.into(),
        page_path: page.into(),
        ord,
        title: title.into(),
        summary: summary.into(),
        body: body.into(),
        start_line: 1,
        end_line: 5,
        source: "structural".into(),
    }
}

#[test]
fn search_finds_chunks_by_identifier_and_cjk() {
    let store = Store::open_in_memory().unwrap();
    store
        .replace_chunks_for_page(
            "01-业务/架构.md",
            "run-1",
            &[
                chunk(
                    "ck_1",
                    "01-业务/架构.md",
                    0,
                    "整体架构",
                    "讲解 pipeline 的主流程与并发控制",
                    "run_init_or_update 负责编排整个 pipeline。",
                ),
                chunk(
                    "ck_2",
                    "01-业务/架构.md",
                    1,
                    "检索",
                    "FTS5 检索实现",
                    "store.search 使用 chunks_fts 虚表。",
                ),
            ],
        )
        .unwrap();

    let hits = store.search("pipeline", 10, SearchMode::Auto).unwrap();
    assert!(
        hits.iter().any(|h| h.id == "ck_1"),
        "expected identifier match, got {hits:?}"
    );
    assert!(hits[0].body.is_some(), "chunk hits must carry the recalled body");

    // CJK phrase longer than the trigram window must also match.
    let cjk = store.search("整体架构", 10, SearchMode::Auto).unwrap();
    assert!(
        cjk.iter().any(|h| h.id == "ck_1"),
        "expected CJK match, got {cjk:?}"
    );
    assert!(store.fts_available(), "FTS5 should be available in bundled SQLite");
}

#[test]
fn search_falls_back_to_like_for_short_terms() {
    let store = Store::open_in_memory().unwrap();
    store
        .replace_chunks_for_page(
            "a.md",
            "run-1",
            &[chunk("ck_1", "a.md", 0, "配置", "配置面", "LLM 配置项说明。")],
        )
        .unwrap();
    let hits = store.search("配置", 10, SearchMode::Like).unwrap();
    assert!(hits.iter().any(|h| h.id == "ck_1"), "LIKE mode must still work");
}

#[test]
fn stale_running_runs_are_reconciled() {
    let store = Store::open_in_memory().unwrap();
    store
        .begin_run("old", "update", Some("p"), Some("m"), Some("zh"))
        .unwrap();
    store.fail_stale_runs(0).unwrap();
    let run = store.get_run("old").unwrap().unwrap();
    assert_eq!(run.status, "failed");
    assert!(run.finished_at.is_some());
}

#[test]
fn page_evidence_hash_roundtrip() {
    let store = Store::open_in_memory().unwrap();
    store
        .upsert_page("quickstart.md", "快速上手", "Quickstart", "d", "bh", "eh", "run-1")
        .unwrap();
    let page = store.get_page("quickstart.md").unwrap().unwrap();
    assert_eq!(page.evidence_hash, "eh");
    assert_eq!(store.count_pages().unwrap(), 1);
}

#[test]
fn prune_drops_pages_whose_file_disappeared() {
    let root = std::env::temp_dir().join(format!("atlas-prune-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("kept.md"), "# kept").unwrap();

    let store = Store::open_in_memory().unwrap();
    for path in ["kept.md", "gone.md"] {
        store
            .upsert_page(path, "t", "Index", "d", "bh", "eh", "run-1")
            .unwrap();
        store
            .replace_chunks_for_page(
                path,
                "run-1",
                &[chunk(&format!("ck_{path}"), path, 0, "t", "s", "body")],
            )
            .unwrap();
    }

    assert_eq!(store.prune_missing_pages(&root).unwrap(), 1);
    assert_eq!(store.count_pages().unwrap(), 1);
    assert_eq!(store.count_chunks().unwrap(), 1);
    assert!(store.get_page("gone.md").unwrap().is_none());
    assert!(
        !store
            .search("body", 10, SearchMode::Auto)
            .unwrap()
            .iter()
            .any(|h| h.page_path.as_deref() == Some("gone.md")),
        "pruned chunks must leave the search index"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn query_terms_splits_cjk_sentences() {
    let terms = query_terms("分块策略是怎么实现的？");
    assert!(
        terms.iter().any(|t| t == "分块策略"),
        "expected the content n-gram, got {terms:?}"
    );
    assert!(
        !terms.iter().any(|t| t.chars().count() > 4),
        "long CJK runs must be windowed, got {terms:?}"
    );
    assert!(
        !terms.iter().any(|t| t == "是怎么"),
        "question filler must be dropped, got {terms:?}"
    );

    // Mixed questions keep identifiers intact.
    let mixed = query_terms("llm.concurrency 和 max_tool_rounds 怎么配？");
    assert!(mixed.iter().any(|t| t == "concurrency"), "got {mixed:?}");
    assert!(
        !mixed.iter().any(|t| t == "和"),
        "single-char fillers must be dropped, got {mixed:?}"
    );

    // A single-character query still searches.
    assert_eq!(query_terms("库"), vec!["库".to_string()]);
}

#[test]
fn search_hits_chunks_for_natural_language_cjk_question() {
    let store = Store::open_in_memory().unwrap();
    store
        .replace_chunks_for_page(
            "04-数据模型/数据模型.md",
            "run-1",
            &[chunk(
                "ck_1",
                "04-数据模型/数据模型.md",
                0,
                "分块",
                "分块策略由模型决定",
                "分块策略由 LLM 决定：先给简要，再给正文。",
            )],
        )
        .unwrap();
    let hits = store
        .search("分块策略是怎么实现的？", 5, SearchMode::Auto)
        .unwrap();
    assert!(
        hits.iter().any(|h| h.id == "ck_1"),
        "a Chinese question must recall the chunk, got {hits:?}"
    );
}
