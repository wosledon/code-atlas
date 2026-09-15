use std::path::Path;

pub(crate) const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".venv", "venv", "__pycache__", ".atlas",
    "atlas", ".idea", ".vscode",
];

/// 极简 gitignore 子集：注释、目录、`*`/`**`、前导 `/`、尾 `/`、`!` 取反。
#[derive(Debug, Default, Clone)]
pub struct IgnoreRules {
    patterns: Vec<IgnorePat>,
}

#[derive(Debug, Clone)]
struct IgnorePat {
    negated: bool,
    dir_only: bool,
    anchored: bool,
    pattern: String,
}

impl IgnoreRules {
    pub fn from_file(path: &Path) -> Self {
        let mut rules = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else {
            return rules;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut negated = false;
            let mut p = line;
            if let Some(rest) = p.strip_prefix('!') {
                negated = true;
                p = rest;
            }
            let mut dir_only = false;
            if let Some(rest) = p.strip_suffix('/') {
                dir_only = true;
                p = rest;
            }
            let mut anchored = false;
            if let Some(rest) = p.strip_prefix('/') {
                anchored = true;
                p = rest;
            } else if p.contains('/') {
                // mid-slash patterns are treated as root-anchored in git
                anchored = true;
            }
            if p.is_empty() || p == "*" {
                continue;
            }
            rules.patterns.push(IgnorePat {
                negated,
                dir_only,
                anchored,
                pattern: p.replace('\\', "/"),
            });
        }
        rules
    }

    pub fn load_root(root: &Path) -> Self {
        let mut rules = Self::default();
        for name in [".gitignore", ".atlasignore", ".ignore"] {
            let p = root.join(name);
            if p.exists() {
                let more = Self::from_file(&p);
                rules.patterns.extend(more.patterns);
            }
        }
        rules
    }

    pub fn is_ignored(&self, rel: &str, is_dir: bool) -> bool {
        let rel = rel.trim_start_matches("./").replace('\\', "/");
        if rel.is_empty() || rel == ".git" {
            return true;
        }
        let mut ignored = false;
        for pat in &self.patterns {
            if pat.dir_only && !is_dir {
                // still allow matching a dir prefix later via path contains
            }
            if glob_match(&pat.pattern, &rel, is_dir || pat.dir_only) {
                ignored = !pat.negated;
            } else if !pat.anchored {
                // unanchored: match any path segment prefix
                let parts: Vec<&str> = rel.split('/').collect();
                for i in 0..parts.len() {
                    let sub = parts[i..].join("/");
                    if glob_match(&pat.pattern, &sub, true) {
                        ignored = !pat.negated;
                        break;
                    }
                    // also match when this segment equals pattern as directory ancestor
                    if glob_match(&pat.pattern, parts[i], true) {
                        ignored = !pat.negated;
                        break;
                    }
                }
            }
        }
        ignored
    }
}

/// 支持 `*`（段内）与 `**`（跨段）的粗匹配。
fn glob_match(pattern: &str, path: &str, _is_dir: bool) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = path.chars().collect();
    fn m(p: &[char], t: &[char]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        if p[0] == '*' {
            if p.len() >= 2 && p[1] == '*' {
                // **
                let mut i = 2;
                while i < p.len() && p[i] == '/' {
                    i += 1;
                }
                if i >= p.len() {
                    return true;
                }
                for k in 0..=t.len() {
                    if m(&p[i..], &t[k..]) {
                        return true;
                    }
                }
                return false;
            }
            // single *
            for k in 0..=t.len() {
                if k < t.len() && t[k] == '/' {
                    break;
                }
                if m(&p[1..], &t[k..]) {
                    return true;
                }
            }
            return false;
        }
        if t.is_empty() {
            return false;
        }
        if p[0] == '?' || p[0] == t[0] {
            return m(&p[1..], &t[1..]);
        }
        false
    }
    m(&pat, &txt) || path.ends_with(&format!("/{pattern}")) || path == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_basic() {
        let rules = IgnoreRules {
            patterns: vec![
                IgnorePat {
                    negated: false,
                    dir_only: false,
                    anchored: false,
                    pattern: "*.log".into(),
                },
                IgnorePat {
                    negated: false,
                    dir_only: true,
                    anchored: false,
                    pattern: "secrets".into(),
                },
                IgnorePat {
                    negated: true,
                    dir_only: false,
                    anchored: false,
                    pattern: "keep.log".into(),
                },
            ],
        };
        assert!(rules.is_ignored("app.log", false));
        assert!(!rules.is_ignored("keep.log", false));
        assert!(rules.is_ignored("secrets/a.txt", false));
        assert!(!rules.is_ignored("src/main.rs", false));
    }
}
