//! Prompt assembly for the two generation passes.
//!
//! Both prompts are built from the same inputs — the per-page outline
//! (`brief::page_brief`), the deterministic repository evidence and the
//! configured output language — so prompt wording can be reviewed and tuned
//! without touching the generation loop in `generate`.
use super::brief::page_brief;
use super::*;

/// Hard format bans that keep tool transcripts out of the wiki.
const FORMAT_BANS: &str = r#"
Output format bans (violations are a failed page):
- Never emit tool-call markup, XML tags like <function=...>, <function_calls>, antml:*, JSON tool schemas, or any transcript of your tool usage. Write only the wiki markdown body.
- Never emit YAML front matter, HTML comments used as generation markers, or "here is the page" preamble.
- Do not paste raw tool arguments or raw command output dumps; summarize with path:line citations instead."#;

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
         3) Then write the page following the required outline exactly.\n\n\
         Structure rules (always apply, regardless of project type):\n\
         - Tables for anything enumerable: files, symbols, commands, interfaces, config keys, stages.\n\
         - mermaid (flowchart / sequenceDiagram / erDiagram / classDiagram) whenever the page \
         explains structure, flow or data relations; the diagram must match the real code.\n\
         - Cover: business/domain problem, concept → code mapping, architecture & boundaries, data \
         model, external interfaces, key flows, how to run/change safely.\n\
         - Module pages: responsibility, key real paths (with line numbers), public entry points, \
         inbound/outbound dependencies, pitfalls, and 上手要点 (what to read/change first).\n\
         - End factual pages with `## Claims` (short verifiable bullets).\n\
         - Cite repository-relative paths in backticks so readers and search can jump to the code.\n\
         - This is a CODEBASE EXPLANATION wiki, not a product marketing site: dense technical \
         writing, no marketing, no filler.\n\
         {}\n\n\
         {}",
        cfg.output.language,
        FORMAT_BANS,
        page_brief(page)
    );
    let user = format!(
        "Page title: {}\nSection type: {}\nFocus: {}\n\n\
         Write the full markdown body only (no YAML front matter, no page-title heading), following \
         the required outline above with real content in every section.\n\n\
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
         rejected by the depth gate: it is too thin for someone maintaining THIS repository. Rewrite \
         it completely and return the improved page.\n\
         Output language: {}.\n\n\
         Rules:\n\
         - Return the full revised markdown body only: no front matter, no diff, no commentary.\n\
         - Keep everything that was already correct, and resolve every listed gap.\n\
         - More depth means more verified specifics — symbol names with line numbers, data \
         fields, error paths, invariants, trade-offs, real commands — never padding, repetition \
         or generic advice. The draft below is your own previous output, so you already read most \
         of the code for it: read further only where a gap needs it, in a single round of \
         read_file / grep calls.\n\
         - Keep the required outline, tables and diagrams; make the diagrams match the real code.\n\
         - If the draft contains tool-call markup or transcripts, strip them entirely; output \
         only clean wiki markdown.\n\n\
         {}\n\n\
         {}",
        cfg.output.language,
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
        assert!(user.contains("## Repository\nreal evidence"), "{user}");

        let (system, user) = expand_messages(&cfg, &page, &["缺少流程图".into()], "draft", "ev");
        assert!(system.contains(&page_brief(&page)), "{system}");
        assert!(user.contains("- 缺少流程图"), "{user}");
        assert!(user.contains("draft"), "{user}");
        assert!(user.contains("ev"), "{user}");

        // 第二遍请求带上草稿，过长时按 12k 字符截断
        let (_, user) = expand_messages(&cfg, &page, &[], &"字".repeat(20_000), "ev");
        assert!(user.contains(&"字".repeat(12_000)));
        assert!(!user.contains(&"字".repeat(12_001)));
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
