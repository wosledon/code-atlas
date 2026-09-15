use crate::types::{OutlineItem, SourceFile};

/// 顶层声明的数量上限，避免超大文件把提示词撑爆。
const MAX_OUTLINE_ITEMS: usize = 200;
/// 缩进超过这个列数的声明视为「函数体内的代码」，不再收录。
const MAX_ITEM_INDENT: usize = 12;
/// 只有「名字后面跟着调用/箭头」这种行才当成方法，且缩进不能太深。
const MAX_CALLABLE_INDENT: usize = 6;

const RUST_MODIFIERS: &[&str] = &[
    "pub(crate)",
    "pub(super)",
    "pub(self)",
    "pub(in crate)",
    "pub ",
    "async ",
    "unsafe ",
    "default ",
    "extern \"C\" ",
    "extern ",
];

const JS_MODIFIERS: &[&str] = &[
    "export ",
    "default ",
    "declare ",
    "abstract ",
    "async ",
    "public ",
    "private ",
    "protected ",
    "readonly ",
    "override ",
    "static ",
];

const JS_KEYWORDS: &[&str] = &[
    "if", "for", "while", "switch", "return", "catch", "else", "case", "await", "new", "throw",
    "typeof", "do", "try", "void", "delete", "yield", "super", "this", "import", "from",
];

/// 比 [`extract_symbols`] 更完整的「文件骨架」：每个声明的种类、名字与**起始行号**。
///
/// 提示词用它给模型一张带行号的地图（`path:line`），模型再按需用 `read_file` 精确读取，
/// 于是不必把整个文件塞进上下文，也不会凭空编造符号名。
pub fn file_outline(file: &SourceFile, source: &str) -> Vec<OutlineItem> {
    let lang = file.language.as_deref().unwrap_or("");
    let mut out: Vec<OutlineItem> = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        if out.len() >= MAX_OUTLINE_ITEMS {
            break;
        }
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with("/*")
            || line.starts_with('*')
        {
            continue;
        }
        let item = match lang {
            "rust" => {
                if line.starts_with("#[") || line.starts_with("#!") {
                    None
                } else {
                    rust_item(line, indent)
                }
            }
            "typescript" | "javascript" => js_item(line, indent),
            "python" => {
                if line.starts_with('#') {
                    None
                } else {
                    py_item(line, indent)
                }
            }
            _ => None,
        };
        if let Some((kind, name)) = item {
            out.push(OutlineItem {
                line: idx + 1,
                kind: kind.to_string(),
                name,
            });
        }
    }
    out
}

fn strip_modifiers<'a>(line: &'a str, modifiers: &[&str]) -> &'a str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start();
        match modifiers.iter().find_map(|m| trimmed.strip_prefix(m)) {
            Some(next) => rest = next,
            None => return trimmed,
        }
    }
}

fn identifier(rest: &str) -> Option<String> {
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || name.len() >= 80 {
        return None;
    }
    Some(name)
}

/// 去掉 `impl<'a, T>` 这类泛型参数前缀。
fn strip_generics(name: &str) -> &str {
    let name = name.trim();
    if !name.starts_with('<') {
        return name;
    }
    let mut depth = 0usize;
    for (i, c) in name.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return name[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    name
}

fn rust_item(line: &str, indent: usize) -> Option<(&'static str, String)> {
    if indent > MAX_ITEM_INDENT {
        return None;
    }
    let rest = strip_modifiers(line, RUST_MODIFIERS);
    if let Some(head) = rest.strip_prefix("impl") {
        let name = head
            .split('{')
            .next()
            .unwrap_or("")
            .split(" where")
            .next()
            .unwrap_or("");
        let name = strip_generics(name).trim();
        return item("impl", name);
    }
    for (kw, kind) in [
        ("const fn ", "fn"),
        ("fn ", "fn"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("macro_rules! ", "macro"),
        ("mod ", "mod"),
        ("type ", "type"),
        ("const ", "const"),
        ("static ", "static"),
        ("union ", "union"),
    ] {
        if let Some(after) = rest.strip_prefix(kw) {
            if kind == "fn" && indent > MAX_CALLABLE_INDENT {
                return None;
            }
            return identifier(after).map(|name| (kind, name));
        }
    }
    None
}

fn js_item(line: &str, indent: usize) -> Option<(&'static str, String)> {
    if indent > MAX_ITEM_INDENT {
        return None;
    }
    let rest = strip_modifiers(line, JS_MODIFIERS);
    for (kw, kind) in [
        ("function ", "function"),
        ("function* ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("type ", "type"),
        ("enum ", "enum"),
        ("const ", "const"),
        ("let ", "let"),
        ("var ", "var"),
    ] {
        if let Some(after) = rest.strip_prefix(kw) {
            let callable = after.contains("=>") || after.contains("function") || kind == "function";
            if matches!(kind, "const" | "let" | "var") && !callable {
                return None;
            }
            return identifier(after).map(|name| (kind, name));
        }
    }
    // 类方法 / 箭头函数组件：`handleClick = ... =>`、`async load(p) {`
    if indent > MAX_CALLABLE_INDENT {
        return None;
    }
    let name = identifier(rest)?;
    if JS_KEYWORDS.contains(&name.as_str()) {
        return None;
    }
    let after = rest[name.len()..].trim_start();
    let callable = after.starts_with('(')
        || (after.starts_with('=')
            && (after.contains("=>") || after.contains("function")))
        || (after.starts_with('<') && after.contains("=>"));
    if callable {
        item("method", &name)
    } else {
        None
    }
}

fn py_item(line: &str, indent: usize) -> Option<(&'static str, String)> {
    if indent > MAX_ITEM_INDENT {
        return None;
    }
    let rest = strip_modifiers(line, &["async "]);
    for (kw, kind) in [("def ", "def"), ("class ", "class")] {
        if let Some(after) = rest.strip_prefix(kw) {
            return identifier(after).map(|name| (kind, name));
        }
    }
    None
}

fn item(kind: &'static str, name: &str) -> Option<(&'static str, String)> {
    if name.is_empty() || name.len() >= 80 {
        None
    } else {
        Some((kind, name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(rel: &str, lang: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from(rel),
            rel: rel.into(),
            language: Some(lang.into()),
            size: 0,
        }
    }

    #[test]
    fn rust_outline_keeps_items_and_line_numbers() {
        let src = "\
//! doc
#[derive(Debug)]
pub struct Foo {
    inner: u8,
}

impl Foo {
    pub fn new() -> Self {
        Self { inner: 0 }
    }

    fn helper(&self) -> u8 {
        let inner = 1;
        inner
    }
}

pub(crate) async fn run(x: u8) -> u8 { x }
";
        let items = file_outline(&file("a.rs", "rust"), src);
        let seen: Vec<String> = items
            .iter()
            .map(|i| format!("{}:{} {}", i.line, i.kind, i.name))
            .collect();
        assert!(seen.contains(&"3:struct Foo".to_string()), "{seen:?}");
        assert!(seen.contains(&"7:impl Foo".to_string()), "{seen:?}");
        assert!(seen.contains(&"8:fn new".to_string()), "{seen:?}");
        assert!(seen.contains(&"12:fn helper".to_string()), "{seen:?}");
        assert!(seen.contains(&"18:fn run".to_string()), "{seen:?}");
        // 函数体里的局部变量不该被当成声明
        assert!(!seen.iter().any(|s| s.contains("inner")), "{seen:?}");
    }

    #[test]
    fn typescript_outline_covers_components_and_hooks() {
        let src = "\
export function useThing() {
  return 1;
}

export const Card = ({ title }: Props) => <div>{title}</div>;

class Store {
  private items: string[] = [];

  async load(id: string) {
    return id;
  }
}

if (foo) {
  bar();
}
";
        let items = file_outline(&file("a.tsx", "typescript"), src);
        let seen: Vec<String> = items
            .iter()
            .map(|i| format!("{}:{} {}", i.line, i.kind, i.name))
            .collect();
        assert!(seen.contains(&"1:function useThing".to_string()), "{seen:?}");
        assert!(seen.contains(&"5:const Card".to_string()), "{seen:?}");
        assert!(seen.contains(&"7:class Store".to_string()), "{seen:?}");
        // 类方法、箭头函数按「名字 + 调用/箭头」识别
        assert!(seen.iter().any(|s| s.ends_with("method load")), "{seen:?}");
        assert!(!seen.iter().any(|s| s.contains("if foo")), "{seen:?}");
    }

    #[test]
    fn python_outline_sees_methods() {
        let src = "\
class Index:
    def add(self, doc):
        pass

async def main():
    pass
";
        let items = file_outline(&file("a.py", "python"), src);
        let seen: Vec<String> = items
            .iter()
            .map(|i| format!("{}:{} {}", i.line, i.kind, i.name))
            .collect();
        assert!(seen.contains(&"1:class Index".to_string()), "{seen:?}");
        assert!(seen.contains(&"2:def add".to_string()), "{seen:?}");
        assert!(seen.contains(&"5:def main".to_string()), "{seen:?}");
    }
}
