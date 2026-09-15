/// Split a natural-language query into safe keywords (max 16, deduped).
///
/// Chinese questions rarely contain spaces, and the FTS5 trigram index only
/// matches literal substrings, so a whole sentence like
/// `分块策略是怎么实现的？` would never hit anything. Long CJK runs are therefore
/// broken into 4-gram / 3-gram windows (matched by the trigram index) plus
/// bigram windows (matched by the `LIKE` fallback, which is what actually makes
/// Chinese recall work when the exact phrase is not in the corpus).
pub fn query_terms(q: &str) -> Vec<String> {
    const MAX_TERMS: usize = 16;
    let mut out: Vec<String> = Vec::new();
    for raw in q.split_whitespace() {
        let cleaned: String = raw
            .chars()
            .filter(|c| !matches!(c, '%' | '_' | '"' | '*' | '\'' | '\\'))
            .collect();
        for token in cleaned.split(|c: char| is_query_separator(c)) {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let len = token.chars().count();
            if is_cjk_heavy(token) && len > 4 {
                for (n, take) in [(4usize, 4usize), (3usize, 4usize), (2usize, 10usize)] {
                    for gram in cjk_windows(token, n).into_iter().take(take) {
                        push_term(&mut out, gram);
                        if out.len() >= MAX_TERMS {
                            return out;
                        }
                    }
                }
                continue;
            }
            // Single characters match far too much to be useful keywords.
            if len < 2 {
                continue;
            }
            push_term(&mut out, token.to_string());
            if out.len() >= MAX_TERMS {
                return out;
            }
        }
    }
    if out.is_empty() {
        // Single-character / symbol-only queries still deserve a search.
        if let Some(raw) = q.split_whitespace().last() {
            push_term(&mut out, raw.trim().to_string());
        }
    }
    out
}

fn push_term(out: &mut Vec<String>, term: String) {
    let term = term.trim().to_string();
    if term.is_empty() {
        return;
    }
    if !out.iter().any(|t| t.eq_ignore_ascii_case(&term)) {
        out.push(term);
    }
}

fn is_query_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '.'
                | ';'
                | ':'
                | '!'
                | '?'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '/'
                | '\\'
                | '|'
                | '#'
                | '+'
                | '='
                | '~'
                | '`'
                | '@'
                | '$'
                | '^'
                | '&'
                | '%'
                | '_'
                | '"'
                | '\''
                | '*'
                // CJK / full-width punctuation
                | '\u{3000}'
                | '\u{FF01}'
                | '\u{FF0C}'
                | '\u{3001}'
                | '\u{3002}'
                | '\u{FF1A}'
                | '\u{FF1B}'
                | '\u{FF1F}'
                | '\u{FF08}'
                | '\u{FF09}'
                | '\u{3010}'
                | '\u{3011}'
                | '\u{300A}'
                | '\u{300B}'
                | '\u{2014}'
                | '\u{2018}'
                | '\u{2019}'
                | '\u{201C}'
                | '\u{201D}'
        )
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |      // kana
        0x3400..=0x9FFF |      // CJK unified ideographs (+ ext A)
        0xAC00..=0xD7AF |      // hangul syllables
        0xF900..=0xFAFF |      // CJK compatibility ideographs
        0x20000..=0x2FA1F     // CJK ext B-F
    )
}

fn is_cjk_heavy(token: &str) -> bool {
    let cjk = token.chars().filter(|c| is_cjk(*c)).count();
    cjk >= 2 && cjk * 2 >= token.chars().count()
}

/// Sliding `n`-char windows, minus windows that are pure question filler.
fn cjk_windows(token: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < n {
        return Vec::new();
    }
    (0..=chars.len() - n)
        .map(|i| chars[i..i + n].iter().collect::<String>())
        .filter(|w| !CJK_FILLER_WINDOWS.contains(&w.as_str()))
        .collect()
}

/// Frequent question/function words that would otherwise match every page.
const CJK_FILLER_WINDOWS: &[&str] = &[
    "怎么", "如何", "什么", "哪些", "哪个", "是否", "可以", "这个", "那个", "我们", "以及", "还有",
    "为什", "么样", "的是", "是个", "在一", "们的", "是一", "在", "的呢", "是的", "有哪些", "是怎么",
    "应该", "需要", "能够", "里面", "关于", "一下", "请问", "为什么",
];

/// Build an FTS5 MATCH expression: quoted terms OR-ed together. Quoting keeps
/// punctuation (dashes, dots, colons) from being parsed as FTS operators.
pub fn fts_query(terms: &[&String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for t in terms {
        let trimmed = t.trim();
        if trimmed.chars().count() < 3 {
            continue;
        }
        parts.push(format!("\"{}\"", trimmed.replace('"', "\"\"")));
    }
    parts.join(" OR ")
}
