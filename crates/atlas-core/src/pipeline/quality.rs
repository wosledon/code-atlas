//! Page body quality gates: pollution detection and safe sanitization.
//!
//! Historical failures left tool-call transcripts (function XML / JSON dumps)
//! and draft markers inside wiki pages. Those bodies must not be reused, must
//! fail the depth gate, and must never be written when the model still emits them.

use super::evidence::strip_front_matter;
use super::*;

/// Substrings that indicate the body is a tool transcript, not a wiki page.
const POLLUTION_MARKERS: &[&str] = &[
    "<function=",
    "<function_calls",
    "</function",
    "antml:",
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

/// Remove obvious tool-transcript lines and draft markers. Residual pollution
/// after sanitization still counts as failure.
pub(crate) fn sanitize_generated_body(body: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.contains("<function=")
            || t.contains("<function_calls")
            || t.contains("</function")
            || t.starts_with("antml:")
            || t.starts_with("tool_use_id")
            || t.contains("assistant to=functions")
        {
            continue;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    s = s.replace(markdown::DRAFT_MARKER, "");
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
}
