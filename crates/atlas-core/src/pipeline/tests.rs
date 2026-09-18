use std::fs;
use std::path::PathBuf;

use super::evidence::{build_evidence, page_fingerprint, read_page_body};
use super::maintenance::first_heading;
use super::write::PageDraft;
use super::*;

use atlas_llm::StreamSink;

/// 最小的真实仓库样本：一个 manifest + 一个有声明行号的源文件。
fn demo_repo(tag: &str) -> (PathBuf, RepoScan) {
    let root = std::env::temp_dir().join(format!("atlas-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\n\n[dependencies]\nanyhow = \"1\"\n",
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub struct Demo;\n\nimpl Demo {\n    pub fn start(&self) -> u8 {\n        1\n    }\n}\n\npub fn helper() {}\n",
    )
    .unwrap();
    let scan = scan_repo(&root).expect("scan");
    (root, scan)
}

fn demo_page() -> PlannedPage {
    PlannedPage {
        rel_path: "02-系统设计/整体架构.md".into(),
        title: "整体架构".into(),
        page_type: "Architecture".into(),
        description: "分层、模块边界与依赖方向".into(),
        tags: vec![],
        module: None,
        focus: "architecture".into(),
    }
}

/// 证据包必须自带代码级锚点：模型即使一次工具都不调，也能看到真实依赖、真实命令、
/// 以及「文件 → 行号 → 符号」的骨架。
#[test]
fn evidence_bundles_dependencies_commands_and_symbol_outline() {
    let (root, scan) = demo_repo("evidence");
    let page = demo_page();
    let evidence = build_evidence(&scan, &page);

    let sections = [
        "## Repository",
        "## Manifests & declared dependencies",
        "## Commands that exist in this repository",
        "## Modules discovered",
        "## Source files in scope",
        "## Source outline (declarations with line numbers)",
        "## Documents this wiki will contain",
    ];
    let mut cursor = 0usize;
    for section in sections {
        let at = evidence
            .find(section)
            .unwrap_or_else(|| panic!("evidence misses `{section}`:\n{evidence}"));
        assert!(at >= cursor, "`{section}` is out of order:\n{evidence}");
        cursor = at;
    }
    assert!(evidence.contains("anyhow"), "{evidence}");
    assert!(evidence.contains("`1` struct Demo"), "{evidence}");
    assert!(evidence.contains("`4` fn start"), "{evidence}");
    assert!(evidence.contains("`9` fn helper"), "{evidence}");

    // 页指纹建立在 evidence 之上，抖动会导致无谓的全量重生成。
    assert_eq!(evidence, build_evidence(&scan, &page));
    let _ = fs::remove_dir_all(&root);
}

/// 运行级 instruction 会写入每页 focus，并进入 salt/fingerprint：换指令应触发全量重算。
#[test]
fn run_instruction_rewrites_focus_and_invalidates_fingerprint() {
    let (root, scan) = demo_repo("instr");
    let base_pages = super::plan::plan_pages(&scan, "update", None);
    let instr_pages = super::plan::plan_pages(&scan, "update", Some("修复污染页面，按当前代码重写"));
    assert!(instr_pages.len() > base_pages.len(), "instruction adds a special-topic page");
    let arch = instr_pages
        .iter()
        .find(|p| p.rel_path == "02-系统设计/整体架构.md")
        .unwrap();
    assert!(arch.focus.contains("Run-level instruction"), "{}", arch.focus);
    assert!(arch.focus.contains("修复污染页面"));

    let cfg = AtlasConfig::default();
    let page = demo_page();
    let mut with_instr = page.clone();
    with_instr.focus = format!("{}\n\nRun-level instruction: 修复污染页面", page.focus);
    let e = build_evidence(&scan, &page);
    let salt_a = super::prompt::prompt_salt(&cfg, &page, "p", "m");
    let salt_b = super::prompt::prompt_salt(&cfg, &with_instr, "p", "m");
    assert_ne!(salt_a, salt_b);
    assert_ne!(
        page_fingerprint(&scan, &page, &e, &salt_a),
        page_fingerprint(&scan, &with_instr, &e, &salt_b)
    );
    let _ = fs::remove_dir_all(&root);
}

/// 污染正文不得进入复用路径：quality 门禁与 depth_gaps 必须拦截工具调用残片。
#[test]
fn polluted_bodies_fail_quality_gate() {
    use super::quality::is_polluted;
    let dirty = "## 职责\n\n<function=read_file>\nkeep\n</function>\n\n## Claims\n- x\n";
    assert!(is_polluted(dirty));
    let gaps = super::brief::depth_gaps(dirty, &demo_page());
    assert!(gaps.iter().any(|g| g.contains("工具调用残片")), "{gaps:?}");
    let cleaned = super::quality::sanitize_generated_body(dirty);
    assert!(!is_polluted(&cleaned));
}

/// 指纹必须把「生成方式」也算进去：改了提示词、换了模型或调了深度开关后，
/// `atlas update` 必须重算这一页，而不是静默复用旧正文。
#[test]
fn page_fingerprint_tracks_the_generation_inputs() {
    let (root, scan) = demo_repo("fingerprint");
    let page = demo_page();
    let evidence = build_evidence(&scan, &page);

    let base = page_fingerprint(&scan, &page, &evidence, "prompt-a");
    assert_eq!(base, page_fingerprint(&scan, &page, &evidence, "prompt-a"));
    assert_ne!(
        base,
        page_fingerprint(&scan, &page, &evidence, "prompt-b"),
        "a different prompt/model salt must invalidate the cached body"
    );
    assert_ne!(
        base,
        page_fingerprint(&scan, &page, &evidence, "prompt-a\n"),
        "evidence appended to the salt must still change the digest"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn first_heading_skips_front_matter_and_fences() {
    assert_eq!(
        first_heading("---\ntitle: x\n---\n\n# 整体架构\n\n正文"),
        Some("整体架构".to_string())
    );
    assert_eq!(
        first_heading("intro\n\n# API Reference"),
        Some("API Reference".to_string())
    );
    assert_eq!(
        first_heading("```sh\n# not a title\n```\n\n# Real"),
        Some("Real".to_string())
    );
    assert_eq!(first_heading("## only h2"), None);
    assert_eq!(first_heading(""), None);
}

/// A page whose evidence is unchanged is re-read from disk and written back
/// verbatim; that round-trip has to be byte-stable, otherwise every run would
/// change the body hash and rebuild the chunks.
#[test]
fn reused_body_round_trips_through_write_and_read() {
    let (root, _scan) = demo_repo("roundtrip");
    let path = root.join("atlas/00-onboarding/概览.md");
    let fm = FrontMatter::new("Onboarding", "概览", "说明", &["a", "b"]);
    let body = "## 概览\n\n正文段落。\n\n```rust\nfn main() {}\n```\n";

    markdown::write_page(&path, &fm, body).unwrap();
    let written = fs::read_to_string(&path).unwrap();
    let reused = read_page_body(&path).expect("body");
    markdown::write_page(&path, &fm, &reused).unwrap();

    assert_eq!(written, fs::read_to_string(&path).unwrap());
    assert!(!written.ends_with("\n\n"));
    let _ = fs::remove_dir_all(&root);
}

/// A page the model is still writing must exist on disk while it is written:
/// the draft carries the front matter, grows with every delta, and drops the
/// narration a round emitted before calling a tool.
#[test]
fn page_draft_streams_to_disk_and_rolls_back_rounds() {
    let (root, _scan) = demo_repo("draft");
    let page = demo_page();
    let path = root.join("atlas").join(&page.rel_path);
    let draft = PageDraft::open(
        path.clone(),
        &FrontMatter::new("Architecture", &page.title, "分层", &["架构"]),
    )
    .unwrap();

    draft.delta("## 概览\n\n第一段。");
    let live = fs::read_to_string(&path).unwrap();
    assert!(live.contains("title: 整体架构"), "{live}");
    assert!(live.contains(markdown::DRAFT_MARKER), "{live}");
    assert!(live.ends_with("第一段。"), "{live}");

    // 工具回合里先说一句、再调用工具：这一轮的文本必须被回滚。
    draft.round_start();
    draft.delta("让我先读一下 src/lib.rs。");
    draft.round_end(false);
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.ends_with("第一段。"), "narration survived: {after}");

    // 失败：文件回到这次运行之前的样子（这里原本不存在，于是草稿被清掉）。
    draft.restore();
    assert!(!path.exists(), "draft left behind: {path:?}");

    let _ = fs::remove_dir_all(&root);
}

/// A draft never replaces the previous real page it is replacing: when the run
/// fails, the page comes back byte-for-byte.
#[test]
fn failed_page_is_restored_to_its_previous_bytes() {
    let (root, _scan) = demo_repo("draft-rollback");
    let page = demo_page();
    let path = root.join("atlas").join(&page.rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let fm = FrontMatter::new("Architecture", &page.title, "分层", &["架构"]);
    markdown::write_page(&path, &fm, "## 概览\n\n上一版正文。").unwrap();
    let before = fs::read(&path).unwrap();

    let draft = PageDraft::open(path.clone(), &fm).unwrap();
    draft.delta("这一次重写……");
    assert!(fs::read_to_string(&path).unwrap().contains("这一次重写"));
    draft.restore();
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "previous page was not restored"
    );

    let _ = fs::remove_dir_all(&root);
}

/// A draft is never reusable: `read_page_body` rejects it, so a killed run is
/// regenerated instead of adopting half a page as the finished one.
#[test]
fn draft_files_are_not_reusable_bodies() {
    let (root, _scan) = demo_repo("draft-reuse");
    let page = demo_page();
    let path = root.join("atlas").join(&page.rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    markdown::write_page(
        &path,
        &FrontMatter::new("Architecture", &page.title, "分层", &["架构"]),
        &format!("{}{}\n\n半页正文", markdown::DRAFT_MARKER, ""),
    )
    .unwrap();
    assert!(read_page_body(&path).is_none(), "draft body was accepted");

    markdown::write_page(
        &path,
        &FrontMatter::new("Architecture", &page.title, "分层", &["架构"]),
        "## 概览\n\n完整正文。",
    )
    .unwrap();
    assert_eq!(
        read_page_body(&path).as_deref().map(str::trim),
        Some("## 概览\n\n完整正文。")
    );

    let _ = fs::remove_dir_all(&root);
}
