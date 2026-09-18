//! Prompt assembly for the two generation passes.
//!
//! Style constraints are distilled from the strongest pages this wiki already
//! produces (Business with entrypoint tables + rules anchors; Module pages with
//! 职责/非职责 + symbol:line APIs; Onboarding with ordered real commands).
//! Both prompts share [`page_brief`] + evidence so wording can be reviewed
//! without touching the generation loop.

use super::brief::page_brief;
use super::*;

/// Hard format bans that keep tool transcripts out of the wiki.
const FORMAT_BANS: &str = r#"
Output format bans (violations are a failed page):
- Never emit tool-call markup, XML tags like <function=...>, <function_calls>, antml:*, JSON tool schemas, or any transcript of your tool usage. Write only the wiki markdown body.
- Never emit YAML front matter, HTML comments used as generation markers, or "here is the page" preamble.
- Do not paste raw tool arguments or raw command output dumps; summarize with path:line citations instead.
- Never invent a line number you did not see in a read_file/outline result; if unsure, cite the symbol name in backticks without ":line"."#;

/// Writing style matched to the best existing wiki pages.
const STYLE_GUIDE: &str = r#"
Writing style (match the best pages already in this wiki; a weak page fails this bar):
1) Open with ONE concrete sentence: what this repo/module/flow IS (or the problem it solves), then immediately WHO uses it and through WHICH real entry (CLI subcommand / HTTP route / MCP tool / fn export).
2) Dense technical prose in the output language. Keep identifiers, paths, crate names, API names, env vars, and CLI flags in code style. No marketing, no filler, no restating the page title as the only content.
3) Tables are the default for anything enumerable. Prefer ≥5 data rows when the repository has that many items; each row should carry a code anchor (`path:line`, symbol, command, config key) — not vague words like "见代码".
4) Module pages must include: 职责 vs 非职责; 关键文件表 (file → role → entry symbol with line); 对外 API 表 (symbol | signature | location); 入边/出边依赖; 数据落在哪里; 易错点与不变量; 上手要点 (read order + safest first change).
5) Business pages must map: problem → users/scenarios with real entrypoints → 领域术语表 (term → meaning → code type/file) → 业务规则表 (rule → file → function:line) → 边界与非目标 → 规则变更的连锁影响.
6) Workflow pages must show: 触发入口 → 分阶段 mermaid → 每阶段输入/输出/副作用表 → 失败与回退 → 并发/锁/幂等 → 调试命令与落盘产物.
7) Mermaid: quote EVERY node/edge label; put real anchors inside labels e.g. core["crates/atlas-core<br/>run_init_or_update"] — never unquoted paths with `/`.
8) Claims: end factual pages with `## Claims` — 5–8 short verifiable bullets; each bullet a fact, preferably with a code anchor. No restating marketing.
9) Cross-link other planned wiki pages when they cover related ground, using relative markdown links from "Documents this wiki will contain".
10) Prefer `path:line` only for files you actually read; otherwise cite the symbol/path without a fabricated line."#;

/// First pass: write the page body from scratch.
pub(super) fn generate_messages(
    cfg: &AtlasConfig,
    page: &PlannedPage,
    evidence: &str,
) -> (String, String) {
    let system = format!(
        "You are Code Atlas, a senior engineer writing an in-repo knowledge wiki that explains an \
         existing codebase so a new maintainer can take over quickly: the depth and structure of a \
         professional repository wiki, not a README summary. The wiki must describe the WHOLE \
         repository, not just one directory.\n\
         Output language: {}.\n\n\
         Working method:\n\
         1) The evidence below ends with a source outline listing real files and their declarations \
         with line numbers. Read the 3–6 files this page actually needs — batch every read_file / \
         grep call into ONE round (the calls in a round run together) and use start_line/end_line \
         so each read stays small — then write. Do this BEFORE writing prose.\n\
         2) Cite what you actually read as `path:line`; never invent files, symbols, routes, tables \
         or config keys.\n\
         3) Then write the page following the required outline exactly.\n\
         4) Follow the writing style and format bans below — they are calibrated to the highest-\
         quality pages this wiki already produces.\n\n\
         {}\n{}\n\n\
         {}",
        cfg.output.language,
        STYLE_GUIDE,
        FORMAT_BANS,
        page_brief(page)
    );
    let user = format!(
        "Page title: {}\nSection type: {}\nFocus: {}\n\n\
         Write the full markdown body only (no YAML front matter, no page-title heading), following \
         the required outline above with real content in every section. Apply the style guide \
         strictly: dense tables with code anchors, correct mermaid quoting, Claims with verifiable \
         facts.\n\n\
         Repository evidence:\n{evidence}",
        page.title, page.page_type, page.focus
    );
    (system, user)
}

/// Second pass: rewrite a draft that the depth gate rejected, resolving each
/// listed gap with verified detail instead of padding.
pub(super) fn expand_messages(
    cfg: &AtlasConfig,
    page: &PlannedPage,
    gaps: &[String],
    draft: &str,
    evidence: &str,
) -> (String, String) {
    let system = format!(
        "You are the editor-in-chief of Code Atlas, an in-repo engineering wiki. A draft page was \
         rejected by the depth gate: it is too thin, generic, or structurally wrong for someone \
         maintaining THIS repository. Rewrite it completely and return the improved page.\n\
         Output language: {}.\n\n\
         Rules:\n\
         - Return the full revised markdown body only: no front matter, no diff, no commentary.\n\
         - Keep everything that was already correct and specific; resolve EVERY listed gap.\n\
         - More depth means more verified specifics — symbol names with line numbers, data \
         fields, error paths, invariants, trade-offs, real commands — never padding, repetition \
         or generic advice. The draft below is your own previous output, so you already read most \
         of the code for it: read further only where a gap needs it, in a single round of \
         read_file / grep calls.\n\
         - Keep the required outline, tables and diagrams; make the diagrams match the real code \
         and quote every mermaid label.\n\
         - If the draft contains tool-call markup or transcripts, strip them entirely.\n\
         - Raise every weak table to code-anchored rows; add 职责/非职责 on Module pages; \
         end with 5–8 verifiable Claims.\n\n\
         {}\n{}\n\n\
         {}",
        cfg.output.language,
        STYLE_GUIDE,
        FORMAT_BANS,
        page_brief(page)
    );
    let draft: String = draft.chars().take(12_000).collect();
    let gap_list = gaps
        .iter()
        .map(|g| format!("- {g}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Page title: {}\nSection type: {}\nFocus: {}\n\n\
         Depth-gate findings that must all be resolved:\n{gap_list}\n\n\
         Current draft:\n{draft}\n\n\
         Repository evidence:\n{evidence}",
        page.title, page.page_type, page.focus
    );
    (system, user)
}

/// Salt mixed into `page_fingerprint` by the caller: the prompt text and the
/// model describe *how* a page was produced, so a tweak to either must
/// invalidate reused bodies. Knobs that only affect *how much* is generated
/// (`depth_pass`, `max_output_tokens`) stay out of the salt — flipping them
/// must not force a full re-generation of every page.
pub(super) fn prompt_salt(
    cfg: &AtlasConfig,
    page: &PlannedPage,
    provider: &str,
    model: &str,
) -> String {
    let (system, _) = generate_messages(cfg, page, "");
    let (expand, _) = expand_messages(cfg, page, &[], "", "");
    format!(
        "{system}\u{1f}{expand}\u{1f}model={provider}/{model}\u{1f}lang={}\u{1f}focus={}\u{1f}desc={}",
        cfg.output.language, page.focus, page.description
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(kind: &str) -> PlannedPage {
        PlannedPage {
            rel_path: "02-系统设计/整体架构.md".into(),
            title: "整体架构".into(),
            page_type: kind.into(),
            description: "分层、模块边界与依赖方向".into(),
            tags: vec![],
            module: None,
            focus: "architecture".into(),
        }
    }

    #[test]
    fn prompts_carry_outline_evidence_and_gaps() {
        let cfg = AtlasConfig::default();
        let page = page("Architecture");

        let (system, user) = generate_messages(&cfg, &page, "## Repository\nreal evidence");
        // 页类型大纲是深度第一道闸门：整段 brief 必须进入 system prompt
        assert!(system.contains(&page_brief(&page)), "{system}");
        assert!(system.contains(&cfg.output.language), "{system}");
        assert!(system.contains("path:line"), "{system}");
        // 样本提炼出的风格约束必须进入 system prompt
        assert!(system.contains("Writing style"), "{system}");
        assert!(system.contains("Claims"), "{system}");
        assert!(system.contains("职责 vs 非职责"), "{system}");
        assert!(system.contains("FORMAT_BANS") || system.contains("Output format bans"), "{system}");
        assert!(user.contains("## Repository\nreal evidence"), "{user}");
        assert!(user.contains("style guide"), "{user}");

        let (system, user) = expand_messages(&cfg, &page, &["缺少流程图".into()], "draft", "ev");
        assert!(system.contains(&page_brief(&page)), "{system}");
        assert!(system.contains("Writing style"), "{system}");
        assert!(user.contains("- 缺少流程图"), "{user}");
        assert!(user.contains("draft"), "{user}");
        assert!(user.contains("ev"), "{user}");

        // 第二遍请求带上草稿，过长时按 12k 字符截断
        let (_, user) = expand_messages(&cfg, &page, &[], &"字".repeat(20_000), "ev");
        assert!(user.contains(&"字".repeat(12_000)));
        assert!(!user.contains(&"字".repeat(12_001)));
    }

    #[test]
    fn module_prompt_demands_responsibility_split() {
        let cfg = AtlasConfig::default();
        let page = page("Module");
        let (system, _) = generate_messages(&cfg, &page, "ev");
        assert!(system.contains("职责 vs 非职责"), "{system}");
        assert!(system.contains("易错点"), "{system}");
        assert!(system.contains("上手要点"), "{system}");
    }

    #[test]
    fn prompt_salt_tracks_prompt_and_model_not_volume_knobs() {
        let cfg = AtlasConfig::default();
        let arch = page("Architecture");
        let base = prompt_salt(&cfg, &arch, "openai-compatible", "m1");
        // 同一输入必须稳定（指纹要可复用），生成方式变了必须失效
        assert_eq!(base, prompt_salt(&cfg, &arch, "openai-compatible", "m1"));
        assert_ne!(base, prompt_salt(&cfg, &arch, "openai-compatible", "m2"));

        // 产量旋钮不进 salt：调 token 上限 / 关深度门不应全量重生成
        let mut more_tokens = cfg.clone();
        more_tokens.llm.max_output_tokens += 1;
        assert_eq!(base, prompt_salt(&more_tokens, &arch, "openai-compatible", "m1"));

        let mut no_depth = cfg.clone();
        no_depth.llm.depth_pass = false;
        assert_eq!(base, prompt_salt(&no_depth, &arch, "openai-compatible", "m1"));

        let mut english = cfg.clone();
        english.output.language = "en".into();
        assert_ne!(base, prompt_salt(&english, &arch, "openai-compatible", "m1"));

        assert_ne!(base, prompt_salt(&cfg, &page("Runbook"), "openai-compatible", "m1"));
    }
}
