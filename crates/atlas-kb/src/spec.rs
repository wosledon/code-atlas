use serde::{Deserialize, Serialize};

/// One retrieval unit of a wiki page: the summary a search hit shows plus the
/// verbatim markdown it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSpec {
    pub title: String,
    pub summary: String,
    pub body: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkMode {
    Structural,
    LlmSemantic,
    Hybrid,
}

impl ChunkMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "structural" => Self::Structural,
            "llm-semantic" => Self::LlmSemantic,
            _ => Self::Hybrid,
        }
    }
}

/// Chunks shorter than `MIN_CHUNK_CHARS` carry too little context to be
/// retrievable on their own, so they are folded into the previous chunk.
const MIN_CHUNK_CHARS: usize = 400;

pub fn merge_small_chunks(chunks: Vec<ChunkSpec>) -> Vec<ChunkSpec> {
    let mut out: Vec<ChunkSpec> = Vec::new();
    for c in chunks {
        let body_len = c.body.trim().chars().count();
        let can_merge = match out.last() {
            Some(prev) => body_len < MIN_CHUNK_CHARS && !prev.body.trim_end().ends_with("```"),
            None => false,
        };
        if can_merge {
            let prev = out.last_mut().unwrap();
            prev.body = format!("{}\n\n{}", prev.body.trim_end(), c.body.trim_start());
            prev.end_line = prev.end_line.max(c.end_line);
            if prev.summary.trim().is_empty() {
                prev.summary = c.summary;
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(title: &str, body: &str) -> ChunkSpec {
        ChunkSpec {
            title: title.into(),
            summary: format!("{title} summary"),
            body: body.into(),
            start_line: 1,
            end_line: 10,
        }
    }

    #[test]
    fn small_chunks_fold_into_the_previous_one() {
        let big = "x".repeat(500);
        let merged = merge_small_chunks(vec![
            spec("A", &big),
            spec("B", "tiny"),
            spec("C", &big),
        ]);
        assert_eq!(merged.len(), 2, "tiny chunk folded: {merged:#?}");
        assert_eq!(merged[0].end_line, 10);
        assert!(merged[0].body.contains("tiny"));
    }

    #[test]
    fn a_leading_small_chunk_is_kept() {
        let merged = merge_small_chunks(vec![spec("intro", "short"), spec("B", &"y".repeat(500))]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].title, "intro");
    }

    #[test]
    fn merge_never_cuts_off_a_code_fence() {
        let fenced = format!("intro\n```rust\n{}\n```", "z".repeat(500));
        let merged = merge_small_chunks(vec![spec("A", &fenced), spec("B", "tail")]);
        assert_eq!(merged.len(), 2, "tail stays separate after a fence");
    }

    #[test]
    fn chunk_mode_parses_known_labels() {
        assert_eq!(ChunkMode::parse("structural"), ChunkMode::Structural);
        assert_eq!(ChunkMode::parse("llm-semantic"), ChunkMode::LlmSemantic);
        assert_eq!(ChunkMode::parse("hybrid"), ChunkMode::Hybrid);
        assert_eq!(ChunkMode::parse("nonsense"), ChunkMode::Hybrid);
    }
}
