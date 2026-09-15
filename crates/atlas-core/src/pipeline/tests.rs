use std::fs;
use std::path::PathBuf;

use super::evidence::{build_evidence, page_fingerprint, read_page_body};
use super::maintenance::first_heading;
use super::*;

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
    assert_eq!(first_heading("intro\n\n# API Reference"), Some("API Reference".to_string()));
    assert_eq!(first_heading("```sh\n# not a title\n```\n\n# Real"), Some("Real".to_string()));
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
