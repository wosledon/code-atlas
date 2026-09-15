use super::spec::{merge_small_chunks, ChunkSpec};

/// Heading / fence aware split used when no model is available (or the model
/// answer cannot be parsed). Boundaries are markdown structure, not semantics.
pub fn structural_chunks(md: &str, target_tokens: usize) -> Vec<ChunkSpec> {
    let approx_tokens = |s: &str| s.chars().count() / 3 + 1;
    let mut chunks = Vec::new();
    let mut current_title = "Overview".to_string();
    let mut buf: Vec<&str> = Vec::new();
    let mut start_line = 1usize;
    let mut line_no = 1usize;
    let mut in_fence = false;

    let mut flush = |title: &str, buf: &mut Vec<&str>, start: usize, end: usize| {
        if buf.is_empty() {
            return;
        }
        let body = buf.join("\n");
        if body.trim().is_empty() {
            buf.clear();
            return;
        }
        let summary = summary_from_body(title, &body);
        chunks.push(ChunkSpec {
            title: title.to_string(),
            summary,
            body,
            start_line: start,
            end_line: end,
        });
        buf.clear();
    };

    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.starts_with('#') {
            flush(&current_title, &mut buf, start_line, line_no.saturating_sub(1));
            current_title = line.trim_start_matches('#').trim().to_string();
            start_line = line_no;
        }
        buf.push(line);
        if approx_tokens(&buf.join("\n")) >= target_tokens && !in_fence {
            flush(&current_title, &mut buf, start_line, line_no);
            start_line = line_no + 1;
        }
        line_no += 1;
    }
    flush(&current_title, &mut buf, start_line, line_no.saturating_sub(1).max(start_line));
    if chunks.is_empty() && !md.trim().is_empty() {
        chunks.push(ChunkSpec {
            title: "Page".into(),
            summary: summary_from_body("Page", md),
            body: md.to_string(),
            start_line: 1,
            end_line: md.lines().count().max(1),
        });
    }
    merge_small_chunks(chunks)
}

/// Reuse the first real prose line as the summary so a search hit is readable
/// even when the model did not write one.
fn summary_from_body(title: &str, body: &str) -> String {
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("```") || t.starts_with("---") {
            continue;
        }
        let s: String = t.chars().take(180).collect();
        return if s.chars().count() >= 20 { s } else { title.to_string() };
    }
    title.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every section body is long enough to survive `merge_small_chunks`.
    fn page() -> String {
        let long = "x".repeat(500);
        format!(
            "# Page\n\nintro {long}\n\n## Section\n\nbody {long}\n\n```rust\nfn x() {{}}\n```\n\n## Next\n\ntail {long}\n"
        )
    }

    #[test]
    fn splits_on_headings_and_keeps_line_spans() {
        let md = page();
        let chunks = structural_chunks(&md, 512);
        let titles: Vec<&str> = chunks.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Page", "Section", "Next"], "{chunks:#?}");
        assert_eq!((chunks[0].start_line, chunks[0].end_line), (1, 4));
        assert_eq!((chunks[1].start_line, chunks[1].end_line), (5, 12));
        assert_eq!((chunks[2].start_line, chunks[2].end_line), (13, 15));
        assert_eq!(chunks[1].body.lines().next().unwrap(), "## Section");
        assert!(chunks.last().unwrap().end_line <= md.lines().count());
        for c in &chunks {
            assert!(c.start_line >= 1 && c.end_line >= c.start_line);
            assert!(!c.summary.trim().is_empty());
        }
    }

    #[test]
    fn empties_never_produce_chunks() {
        assert!(structural_chunks("", 512).is_empty());
        assert!(structural_chunks("\n\n", 512).is_empty());
    }

    #[test]
    fn target_size_splits_a_long_section_that_has_no_headings() {
        let md = (0..200)
            .map(|i| format!("line {i} with a bit of prose to add up"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = structural_chunks(&md, 512);
        assert!(chunks.len() > 1, "expected size-based split: {chunks:#?}");
        assert_eq!(chunks[0].start_line, 1);
        assert!(chunks[1].start_line > chunks[0].start_line);
        assert!(chunks[0].body.chars().count() >= 400, "not folded away");
    }
}
