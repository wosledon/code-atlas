use anyhow::Result;
use atlas_llm::LlmClient;
use atlas_store::{ChunkRow, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

pub async fn semantic_chunk_page(
    llm: &LlmClient,
    page_title: &str,
    md: &str,
    target_tokens: usize,
) -> Result<(Vec<ChunkSpec>, bool)> {
    if llm.is_host_agent() {
        return Ok((structural_chunks(md, target_tokens), false));
    }
    let system = "You are a knowledge-base indexer. You decide how a technical wiki page is split for retrieval. \
Return ONLY a JSON array (no prose, no code fences). Each item: \
{\"title\": string, \"summary\": string, \"body\": string, \"start_line\": number, \"end_line\": number}. \
Rules: \
(1) Each chunk must be self-contained: a reader who only sees that chunk can act on it. \
(2) \"summary\" is 1-2 sentences stating what the chunk explains and when it is relevant. \
(3) \"body\" is the verbatim markdown slice (keep code fences, tables, lists intact). \
(4) Split on topic boundaries, not on fixed sizes; aim for 300-700 tokens per chunk and between 2 and 10 chunks. \
(5) Never invent, translate or summarise away content, and never drop code blocks. \
(6) \"start_line\"/\"end_line\" are 1-based line numbers inside the given markdown.";
    let user = format!(
        "Page title: {page_title}\nTarget chunk size: ~{target_tokens} tokens.\n\n\
         Split this markdown into semantic chunks:\n\n```markdown\n{md}\n```" 
    );
    let resp = llm.chat(system, &user).await?;
    match parse_chunk_json(&resp.text) {
        Some(specs) if !specs.is_empty() => Ok((merge_small_chunks(specs), true)),
        _ => Ok((structural_chunks(md, target_tokens), false)),
    }
}

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
    if out.is_empty() { None } else { Some(out) }
}

pub fn store_chunks(
    store: &Store,
    page_path: &str,
    run_id: &str,
    specs: &[ChunkSpec],
    source: &str,
) -> Result<()> {
    let rows: Vec<ChunkRow> = specs
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut h = Sha256::new();
            h.update(page_path.as_bytes());
            h.update(i.to_string().as_bytes());
            h.update(c.body.as_bytes());
            let id = format!("ck_{}", &hex::encode(h.finalize())[..12]);
            ChunkRow {
                id,
                page_path: page_path.to_string(),
                ord: i as i64,
                title: c.title.clone(),
                summary: c.summary.clone(),
                body: c.body.clone(),
                start_line: c.start_line as i64,
                end_line: c.end_line as i64,
                source: source.to_string(),
            }
        })
        .collect();
    store.replace_chunks_for_page(page_path, run_id, &rows)?;
    Ok(())
}

pub fn upsert_module_entities(store: &Store, run_id: &str, modules: &[(String, String)]) -> Result<()> {
    // modules: (canonical_name, path)
    for (name, path) in modules {
        let key = format!("module:{name}");
        store.upsert_entity(
            "module",
            name,
            &key,
            &serde_json::json!({ "path": path }),
            "active",
            run_id,
        )?;
    }
    Ok(())
}

pub fn link_page_to_module(store: &Store, run_id: &str, page: &str, module_name: &str) -> Result<()> {
    let page_key = format!("page:{}", page);
    let page_id = store.upsert_entity(
        "page",
        page,
        &page_key,
        &serde_json::json!({ "path": page }),
        "active",
        run_id,
    )?;
    let module_key = format!("module:{module_name}");
    let module_id = store.upsert_entity(
        "module",
        module_name,
        &module_key,
        &serde_json::json!({}),
        "active",
        run_id,
    )?;
    store.upsert_relation(&page_id, &module_id, "describes", &serde_json::json!({}), 1.0, run_id)?;
    Ok(())
}
