//! Page body quality gates: pollution detection and safe sanitization.
//!
//! Historical failures left tool-call transcripts (function XML / JSON dumps)
//! and draft markers inside wiki pages. Those bodies must not be reused, must
//! fail the depth gate, and must never be written when the model still emits them.

use super::evidence::strip_front_matter;
use super::*;

/// Substrings that indicate the body is a tool transcript, not a wiki page.
/// The second half covers the plain-text dialects models fall back to when they
/// want a tool but the protocol is not offered (`<tool_call><function=...>`).
const POLLUTION_MARKERS: &[&str] = &[
    "<function=",
    "<function_calls",
    "</function",
    "antml:",
    "<tool_call",
    "</tool_call",
    "<parameter=",
    "</parameter",
    "<tool_use",
    "<invoke name=",
    "tool_use_id",
    "assistant to=functions",
    "{\"name\":\"read_file\"",
    "{\"name\":\"grep\"",
    "atlas:draft",
];

/// Returns the first pollution marker found, if any.
pub(crate) fn body_pollution(body: &str) -> Option<&'static str> {
    let stripped = strip_front_matter(body);
    if stripped.contains(markdown::DRAFT_MARKER) {
        return Some(markdown::DRAFT_MARKER);
    }
    for marker in POLLUTION_MARKERS {
        if stripped.contains(marker) {
            return Some(marker);
        }
    }
    // Repeated raw function-call lines are a transcript even without exact markers.
    let call_lines = stripped
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("<function") || t.starts_with("antml:function")
        })
        .count();
    if call_lines >= 2 {
        return Some("<function* transcript lines");
    }
    None
}

pub(crate) fn is_polluted(body: &str) -> bool {
    body_pollution(body).is_some()
}

/// Remove tool transcripts and draft markers.
///
/// The text dialects (`<tool_call><function=…`) are removed by the same code the
/// model client uses to parse them, so a dialect fix cannot land in one place and
/// be missed here. What that code cannot close stays in the body on purpose:
/// [`body_pollution`] then fails the page instead of shipping a truncated one.
pub(crate) fn sanitize_generated_body(body: &str) -> String {
    let stripped = atlas_llm::strip_text_tool_calls(body);
    let mut out: Vec<&str> = stripped
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.contains("{\"name\":\"read_file\"") && !t.contains("{\"name\":\"grep\"")
        })
        .collect();
    out.retain(|l| !l.contains(markdown::DRAFT_MARKER));
    let mut s = out.join("\n");
    // Collapse runs of blank lines left by stripped transcript blocks.
    while s.contains("\n\n\n") {
        s = s.replace("\n\n\n", "\n\n");
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tool_transcript_pollution() {
        let clean = "## 职责\n\n`crates/core/src/lib.rs:1` 定义入口。\n";
        assert!(body_pollution(clean).is_none());

        let dirty = "## 职责\n\n<function=read_file>\n<path>x</path>\n</function>\n正文\n";
        assert!(body_pollution(dirty).is_some());

        let draft = format!("intro\n{}\nrest", markdown::DRAFT_MARKER);
        assert!(body_pollution(&draft).is_some());
    }

    #[test]
    fn sanitize_strips_transcript_lines() {
        let body = "## 标题\n\n<function=grep>\nkeep this\nantml:invoke\n";
        let cleaned = sanitize_generated_body(body);
        assert!(cleaned.contains("keep this"));
        assert!(!cleaned.contains("<function"));
        assert!(!cleaned.contains("antml:"));
        assert!(body_pollution(&cleaned).is_none());
    }

    /// 文本方言工具调用（模型把 `tool_calls` 写成 `<tool_call>` 正文）连同参数一起删除，
    /// 不能只删 `<function=...>` 那一行、把路径和参数值留在页面里。
    #[test]
    fn sanitize_drops_whole_tool_call_blocks() {
        let body = "\
## 职责

正文开头。

<tool_call>
<function=read_file>
<parameter=path>
crates/atlas-store/src/schema.rs
</parameter>
<parameter=start_line>
121
</parameter>
</function>
</tool_call>
<tool_call>
<function=read_file>
<parameter=path>
crates/atlas-store/src/entities.rs
</parameter>
</function>
</tool_call>

## 结尾

正文结尾。
";
        assert!(body_pollution(body).is_some());
        let cleaned = sanitize_generated_body(body);
        assert!(cleaned.contains("正文开头。"));
        assert!(cleaned.contains("## 结尾"));
        assert!(cleaned.contains("正文结尾。"));
        assert!(!cleaned.contains("atlas-store"), "{cleaned}");
        assert!(!cleaned.contains("parameter"), "{cleaned}");
        assert!(body_pollution(&cleaned).is_none(), "{cleaned}");
    }

    /// 没闭合的块只能删到自己那一行为止：残留的参数行必须继续触发门禁，
    /// 否则整页会被静默截断后当成合格正文写入。
    #[test]
    fn unclosed_tool_call_block_still_fails_the_gate() {
        let body = "正文开头。\n\n<tool_call>\n<function=read_file>\n<parameter=path>\ncrates/x.rs\n</parameter>\n";
        let cleaned = sanitize_generated_body(body);
        assert!(cleaned.contains("正文开头。"), "{cleaned}");
        assert!(!cleaned.contains("<function"), "{cleaned}");
        assert!(body_pollution(&cleaned).is_some(), "{cleaned}");
    }

    #[test]
    fn sanitize_keeps_single_line_blocks_tidy() {
        let body = "前文\n<tool_call><function=grep></function></tool_call>\n后文\n";
        let cleaned = sanitize_generated_body(body);
        assert_eq!(cleaned, "前文\n\n后文");
    }
}
