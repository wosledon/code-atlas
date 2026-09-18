use std::path::Path;

use crate::types::SourceFile;

/// Lightweight deterministic "symbols" for MVP: public fns / classes / exports.
pub fn extract_symbols(file: &SourceFile, source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lang = file.language.as_deref().unwrap_or("");
    match lang {
        "rust" => {
            for line in source.lines() {
                let t = line.trim();
                if (t.starts_with("pub fn ") || t.starts_with("pub struct ") || t.starts_with("pub enum "))
                    && let Some(name) = after_keyword(t) {
                        out.push((t.split_whitespace().nth(1).unwrap_or("").to_string(), name));
                    }
            }
        }
        "typescript" | "javascript" => {
            for line in source.lines() {
                let t = line.trim();
                if (t.starts_with("export function ")
                    || t.starts_with("export class ")
                    || t.starts_with("export interface ")
                    || t.starts_with("export type ")
                    || t.starts_with("export const "))
                    && let Some(name) = after_keyword(t) {
                        out.push((t.split_whitespace().nth(1).unwrap_or("").to_string(), name));
                    }
            }
        }
        "python" => {
            for line in source.lines() {
                let t = line.trim();
                if (t.starts_with("def ") || t.starts_with("class "))
                    && let Some(name) = after_keyword(t) {
                        out.push((t.split_whitespace().next().unwrap_or("").to_string(), name));
                    }
            }
        }
        _ => {}
    }
    out.into_iter()
        .filter(|(_, n)| !n.is_empty() && n.len() < 80)
        .collect()
}

fn after_keyword(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let mut tok = parts.next()?;
    // skip modifiers
    while matches!(tok, "pub" | "export" | "async") {
        tok = parts.next()?;
    }
    // skip kind keyword if present
    if matches!(
        tok,
        "fn" | "def" | "class" | "struct" | "enum" | "function" | "interface" | "type" | "const"
    ) {
        // name follows
    } else if matches!(tok, "export" | "pub") {
        // already handled
    }
    let name = if matches!(
        tok,
        "fn" | "def" | "class" | "struct" | "enum" | "function" | "interface" | "type" | "const"
    ) {
        parts.next()?
    } else {
        tok
    };
    let name = name.trim_end_matches(['(', ':', '<', '{']).to_string();
    Some(name)
}

pub fn read_file_excerpt(path: &Path, max_bytes: usize) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    // Never cut inside a multi-byte character: back off to a UTF-8 boundary.
    let mut end = bytes.len().min(max_bytes);
    while end > 0 && (bytes[end - 1] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    if end == 0 && !bytes.is_empty() && bytes.len() <= max_bytes {
        // whole file is smaller than the window; keep as-is
        end = bytes.len();
    }
    String::from_utf8_lossy(&bytes[..end]).to_string()
}
