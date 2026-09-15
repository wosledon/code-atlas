use anyhow::Result;
use atlas_llm::LlmClient;

use super::spec::{merge_small_chunks, ChunkSpec};
use super::structural::structural_chunks;

/// The chunker is a second, independent LLM task: it never rewrites the page,
/// it only decides where the retrieval boundaries are.
const SYSTEM_PROMPT: &str = "You are a knowledge-base indexer. You decide how a technical wiki page is split for retrieval. \
Return ONLY a JSON array (no prose, no code fences). Each item: \
{\"title\": string, \"summary\": string, \"body\": string, \"start_line\": number, \"end_line\": number}. \
Rules: \
(1) Each chunk must be self-contained: a reader who only sees that chunk can act on it. \
(2) \"summary\" is 1-2 sentences stating what the chunk explains and when it is relevant. \
(3) \"body\" is the verbatim markdown slice (keep code fences, tables, lists intact). \
(4) Split on topic boundaries, not on fixed sizes; aim for 300-700 tokens per chunk and between 2 and 10 chunks. \
(5) Never invent, translate or summarise away content, and never drop code blocks. \
(6) \"start_line\"/\"end_line\" are 1-based line numbers inside the given markdown.";

pub(crate) fn system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

/// Ask the model to cut `md` into retrieval units. Returns `(chunks, used_llm)`;
/// `used_llm == false` means the structural fallback was used (unparseable
/// answer, or a host-agent provider that has no model behind it).
pub async fn semantic_chunk_page(
    llm: &LlmClient,
    page_title: &str,
    md: &str,
    target_tokens: usize,
) -> Result<(Vec<ChunkSpec>, bool)> {
    if llm.is_host_agent() {
        return Ok((structural_chunks(md, target_tokens), false));
    }
    let user = format!(
        "Page title: {page_title}\nTarget chunk size: ~{target_tokens} tokens.\n\n\
         Split this markdown into semantic chunks:\n\n```markdown\n{md}\n```"
    );
    let resp = llm.chat(system_prompt(), &user).await?;
    match parse_chunk_json(&resp.text) {
        Some(specs) if !specs.is_empty() => Ok((merge_small_chunks(specs), true)),
        _ => Ok((structural_chunks(md, target_tokens), false)),
    }
}

/// Tolerant JSON reader: models like to wrap the array in prose or fences, so
/// slice between the first `[` and the last `]` and skip malformed items.
fn parse_chunk_json(text: &str) -> Option<Vec<ChunkSpec>> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    let slice = &text[start..=end];
    let v: serde_json::Value = serde_json::from_str(slice).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::new();
    for item in arr {
        let body = item["body"].as_str().unwrap_or_default().trim().to_string();
        if body.is_empty() {
            continue;
        }
        let title = item["title"].as_str().unwrap_or("Chunk").trim().to_string();
        let summary = item["summary"].as_str().unwrap_or_default().trim().to_string();
        let summary = if summary.is_empty() { title.clone() } else { summary };
        out.push(ChunkSpec {
            title,
            summary,
            body,
            start_line: item["start_line"].as_u64().unwrap_or(1) as usize,
            end_line: item["end_line"].as_u64().unwrap_or(1) as usize,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_array_wrapped_in_prose_and_fences() {
        let raw = "Sure, here you go:\n```json\n[{\"title\":\"T\",\"summary\":\"s\",\"body\":\"b\",\"start_line\":2,\"end_line\":4}]\n```";
        let specs = parse_chunk_json(raw).expect("array parsed");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].start_line, 2);
        assert_eq!(specs[0].end_line, 4);
    }

    #[test]
    fn drops_items_without_body_and_defaults_title() {
        let raw = r#"[{"body":"   "},{"summary":"only summary","body":"kept"}]"#;
        let specs = parse_chunk_json(raw).expect("one usable item");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].title, "Chunk");
        assert_eq!(specs[0].summary, "only summary");
    }

    #[test]
    fn garbage_returns_none_so_the_caller_falls_back() {
        assert!(parse_chunk_json("no json at all").is_none());
        assert!(parse_chunk_json("[]").is_none());
    }

    #[test]
    fn prompt_pins_the_output_contract() {
        let p = system_prompt();
        assert!(p.contains("Return ONLY a JSON array"), "{p}");
        assert!(p.contains("\"start_line\"") && p.contains("\"end_line\""));
    }
}
