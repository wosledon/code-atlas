//! Glob matcher and shared path/text helpers for repo tools.

use std::path::Path;

pub(crate) const MAX_LINE_CHARS: usize = 400;

pub(crate) fn is_skipped_dir(path: &Path) -> bool {
    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    path.is_dir()
        && matches!(
            name.as_str(),
            ".git" | "node_modules" | "target" | "dist" | ".venv" | "venv" | "__pycache__" | ".atlas-data"
        )
}

pub(crate) fn is_text_extension(rel: &str) -> bool {
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "rs" | "toml" | "json" | "md" | "txt" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java"
            | "c" | "h" | "cpp" | "hpp" | "cs" | "rb" | "php" | "sh" | "ps1" | "bat" | "yml" | "yaml"
            | "sql" | "css" | "scss" | "html" | "vue" | "kt" | "swift" | "gradle" | "mod" | "sum"
            | "cfg" | "ini" | "xml" | "proto" | "mjs" | "cjs" | "lock" | "env" | "gitignore"
    )
}

pub(crate) fn clip(line: &str) -> String {
    let t = line.trim_end();
    if t.chars().count() <= MAX_LINE_CHARS {
        return t.to_string();
    }
    let mut s: String = t.chars().take(MAX_LINE_CHARS).collect();
    s.push('…');
    s
}

/// Glob matcher with `**` (any depth), `*` (within one segment) and `?`.
pub fn shell_glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (
        pattern.replace('\\', "/").chars().collect(),
        text.replace('\\', "/").chars().collect(),
    );
    glob_chars(&p, &t)
}

fn glob_chars(pat: &[char], txt: &[char]) -> bool {
    if pat.is_empty() {
        return txt.is_empty();
    }
    match pat[0] {
        '*' if pat.len() > 1 && pat[1] == '*' => {
            let rest = &pat[2..];
            if rest.first() == Some(&'/') && glob_chars(&rest[1..], txt) {
                return true;
            }
            (0..=txt.len()).any(|i| glob_chars(rest, &txt[i..]))
        }
        '*' => (0..=txt.len())
            .take_while(|i| *i == 0 || txt[i - 1] != '/')
            .any(|i| glob_chars(&pat[1..], &txt[i..])),
        '?' => match txt.first() {
            Some(c) if *c != '/' => glob_chars(&pat[1..], &txt[1..]),
            _ => false,
        },
        c => match txt.first() {
            Some(t) if *t == c => glob_chars(&pat[1..], &txt[1..]),
            _ => false,
        },
    }
}
