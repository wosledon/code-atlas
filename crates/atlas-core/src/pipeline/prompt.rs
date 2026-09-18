//! Prompt assembly for the two generation passes.
//!
//! Style constraints are distilled from the strongest pages this wiki already
//! produces (Business with entrypoint tables + rules anchors; Module pages with
//! 职责/非职责 + symbol:line APIs; Onboarding with ordered real commands).
//! Both prompts share [`page_brief`] + evidence so wording can be reviewed
//! without touching the generation loop.
//!
//! **Layout is cache-critical.** Providers bill prompt caching by *prefix*, so
//! the request is ordered most-stable-first: the system message is byte-identical
//! for every page (and every pass), then the message opens with the run-shared
//! evidence, and only then the page-specific parts. The depth rewrite reuses the
//! first pass's `system` + `user` verbatim and appends its own section, so the
//! second call of a page hits the prefix the first call just paid for.

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

/// Task framing plus working method. Reads the same for every page, so it lives
/// at the very front of the request where a provider's prompt cache can keep it.
fn static_task_framing() -> String {
    "You are Code Atlas, a senior engineer writing an in-repo knowledge wiki that explains an \
     existing codebase so a new maintainer can take over quickly: the depth and structure of a \
     professional repository wiki, not a README summary. The wiki must describe the WHOLE \
     repository, not just one directory.\n\n\
     Working method:\n\
     1) The evidence below ends with a source outline listing real files and their declarations \
     with line numbers. Read the 3–6 files this page actually needs — batch every read_file / \
     grep call into ONE round (the calls in a round run together) and use start_line/end_line \
     so each read stays small — then write. Do this BEFORE writing prose.\n\
     2) Cite what you actually read as `path:line`; never invent files, symbols, routes, tables \
     or config keys.\n\
     3) Then write the page following the required outline exactly.\n\
     4) Follow the writing style and format bans below — they are calibrated to the highest-\
     quality pages this wiki already produces.\n"
        .to_string()
}

/// The system message: identical for every page in a run (and across runs while
/// the prompt text is unchanged), which is what makes the cache prefix long.
pub(super) fn static_system(cfg: &AtlasConfig) -> String {
    format!(
        "{}Output language: {}.\n\n{}\n{}\n",
        static_task_framing(),
        cfg.output.language,
        STYLE_GUIDE,
        FORMAT_BANS
    )
}

/// The page-specific message: run-shared evidence first (identical for every
/// page), page scope next, and the per-page instructions last — closest to where
/// the model starts writing.
fn page_user(page: &PlannedPage, evidence: &str) -> String {
    format!(
        "Repository evidence:\n{evidence}\n\n---\n\
         Page title: {}\nSection type: {}\nFocus: {}\n\n\
         {}\n\n\
         Write the full markdown body only (no YAML front matter, no page-title heading), following \
         the required outline above with real content in every section. Apply the style guide \
         strictly: dense tables with code anchors, correct mermaid quoting, Claims with verifiable \
         facts.",
        page.title,
        page.page_type,
        page.focus,
        page_brief(page)
    )
}

/// First pass: write the page body from scratch.
pub(super) fn generate_messages(
    cfg: &AtlasConfig,
    page: &PlannedPage,
    evidence: &str,
) -> (String, String) {
    (static_system(cfg), page_user(page, evidence))
}

/// Second pass: rewrite a draft that the depth gate rejected, resolving each
/// listed gap with verified detail instead of padding.
///
/// The system and the first part of the user message are exactly what the first
/// pass sent, so a provider that caches prompt prefixes can serve that whole
/// span from cache and bill only the gap list + draft.
pub(super) fn expand_messages(
    cfg: &AtlasConfig,
    page: &PlannedPage,
    gaps: &[String],
    draft: &str,
    evidence: &str,
) -> (String, String) {
    let system = static_system(cfg);
    let draft: String = draft.chars().take(12_000).collect();
    let gap_list = gaps
        .iter()
        .map(|g| format!("- {g}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "{}\n\n---\n\
         A first pass at this page was rejected by the depth gate: too thin, generic, or \
         structurally wrong for someone maintaining THIS repository. Rewrite it completely and \
         return the improved page.\n\
         Rules:\n\
         - Return the full revised markdown body only: no front matter, no diff, no commentary.\n\
         - Keep everything that was already correct and specific; resolve EVERY listed gap.\n\
         - More depth means more verified specifics — symbol names with line numbers, data \
         fields, error paths, invariants, trade-offs, real commands — never padding, repetition \
         or generic advice. You already read most of the code for the draft: read further only \
         where a gap needs it, in a single round of read_file / grep calls.\n\
         - Keep the required outline, tables and diagrams; make the diagrams match the real code \
         and quote every mermaid label.\n\
         - If the draft contains tool-call markup or transcripts, strip them entirely.\n\
         - Raise every weak table to code-anchored rows; add 职责/非职责 on Module pages; end with \
         5–8 verifiable Claims.\n\n\
         Depth-gate findings that must all be resolved:\n{gap_list}\n\n\
         First-pass draft:\n{draft}\n\n\
         Return the full revised page now.",
        page_user(page, evidence)
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
    // The whole prompt prefix, not just the system message: the page brief and
    // the page header now live in the user message, and editing either must
    // still invalidate a reused body.
    let (system, user) = generate_messages(cfg, page, "");
    format!("{system}\u{1f}{user}\u{1f}model={provider}/{model}")
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
        let brief = page_brief(&page);

        let (system, user) = generate_messages(&cfg, &page, "## Repository\nreal evidence");
        // 页类型大纲是深度第一道闸门：整段 brief 必须进去（放在页面专属消息里，
        // 因为 system 要跨页逐字节一致，供应商的 prompt 缓存才能命中）
        assert!(user.contains(&brief), "{user}");
        assert!(system.contains(&cfg.output.language), "{system}");
        assert!(system.contains("path:line"), "{system}");
        // 样本提炼出的风格约束必须进入 system prompt
        assert!(system.contains("Writing style"), "{system}");
        assert!(system.contains("Claims"), "{system}");
        assert!(system.contains("职责 vs 非职责"), "{system}");
        assert!(system.contains("FORMAT_BANS") || system.contains("Output format bans"), "{system}");
        // 缓存关键：system 不含任何页级内容
        assert!(!system.contains(&page.title), "{system}");
        assert!(!system.contains(&brief), "{system}");
        assert!(user.contains("## Repository\nreal evidence"), "{user}");
        assert!(user.contains("style guide"), "{user}");

        let (expand_system, expand_user) = expand_messages(
            &cfg,
            &page,
            &["缺少流程图".into()],
            "draft",
            "## Repository\nreal evidence",
        );
        // 深度重写与首稿共享整段前缀，第二遍只多付「缺口 + 草稿」
        assert_eq!(expand_system, system);
        assert!(
            expand_user.starts_with(&user),
            "prefix must match the first pass"
        );
        assert!(expand_user.contains(&brief), "{expand_user}");
        assert!(expand_user.contains("- 缺少流程图"), "{expand_user}");
        assert!(expand_user.contains("draft"), "{expand_user}");
        assert!(expand_user.contains("real evidence"), "{expand_user}");

        // 第二遍请求带上草稿，过长时按 12k 字符截断
        let (_, user) = expand_messages(&cfg, &page, &[], &"字".repeat(20_000), "ev");
        assert!(user.contains(&"字".repeat(12_000)));
        assert!(!user.contains(&"字".repeat(12_001)));
    }

    /// 同一 run 里每一页的 system 必须逐字节一致，页级内容不得混进去。
    #[test]
    fn system_prompt_is_identical_across_pages() {
        let cfg = AtlasConfig::default();
        let (a, _) = generate_messages(&cfg, &page("Architecture"), "ev-a");
        let (b, _) = generate_messages(&cfg, &page("Module"), "ev-b");
        assert_eq!(a, b);

        let mut other_repo = cfg.clone();
        other_repo.output.language = "en".into();
        let (c, _) = generate_messages(&other_repo, &page("Architecture"), "ev-a");
        assert_ne!(a, c, "输出语言变了，前缀就该变");
    }

    /// 共享证据必须排在页级内容之前：前缀越长的公共段，缓存越省。
    /// （section 顺序在 `pipeline::tests::evidence_bundles_...` 里也有断言。）
    #[test]
    fn page_user_puts_instructions_last() {
        let cfg = AtlasConfig::default();
        let page = page("Module");
        let (_, user) = generate_messages(&cfg, &page, "## Repository\nshared head");
        let head = user.find("## Repository").expect("evidence first");
        let brief = user.find(&page_brief(&page)).expect("brief present");
        assert!(head < brief, "证据在前、页级写作要求在后：{user}");
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
