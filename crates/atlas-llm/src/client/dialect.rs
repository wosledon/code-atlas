//! 文本方言工具调用：模型没走 `tool_calls[]`、而是把调用写成正文时的处理。
//!
//! 触发场景固定：请求里没有提供工具（最后一轮、provider 不支持工具、工具回合失败后
//! 退回普通问答），或提供了工具但模型仍用文本格式作答。不处理它，这段转录就会以
//! 「页面正文」的身份落进 wiki。
//!
//! 两件事在这里闭环：
//! - [`resolve`] 把一轮回答还原成（真调用, 正文），调用方据此执行工具回合；
//! - [`strip`] 把无法执行的转录从文本里摘掉——`atlas-core` 的页面质量门用的是同一实现。

use crate::types::{ToolCall, ToolSpec};
use serde_json::{Map, Value};

/// Opening/closing pair of every container a text tool call is wrapped in.
/// `<function=` doubles as the opening when the model skips the `<tool_call>`
/// wrapper, so it is listed last.
const BLOCKS: &[(&str, &str)] = &[
    ("<tool_calls", "</tool_calls>"),
    ("<tool_call", "</tool_call>"),
    ("<function_calls", "</function_calls>"),
    ("<function_call", "</function_call>"),
    ("<tool_use", "</tool_use>"),
    ("<invoke name=", "</invoke>"),
    ("antml:invoke", "</antml:invoke>"),
    ("<function=", "</function>"),
];

/// Earliest container in `text`: byte offset, opening and closing marker.
fn find_block(text: &str) -> Option<(usize, &'static str, &'static str)> {
    let mut best: Option<(usize, &'static str, &'static str)> = None;
    for (open, close) in BLOCKS {
        let Some(at) = text.find(open) else { continue };
        // Same offset: the longer marker is the more specific container.
        let better = match best {
            Some((at_best, open_best, _)) => {
                at < at_best || (at == at_best && open.len() > open_best.len())
            }
            None => true,
        };
        if better {
            best = Some((at, open, close));
        }
    }
    best
}

/// Lines that are transcript on their own, with no container left to strip.
fn is_transcript_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("antml:")
        || t.starts_with("tool_use_id")
        || t.contains("<function=")
        || t.contains("<function_call")
        || t.contains("</function")
        || t.contains("<tool_call")
        || t.contains("</tool_call")
        || t.contains("<tool_use")
        || t.contains("</tool_use")
        || t.contains("<invoke name=")
        || t.contains("</invoke")
        || t.contains("assistant to=functions")
}

/// Remove every text-dialect call from `text`; clean text passes through byte
/// for byte.
///
/// A closed container goes as a whole (its `<parameter=…>` lines and their raw
/// values with it). An unclosed one is only dropped up to the end of its own
/// line: what it left behind stays visible to the caller's pollution gate, which
/// fails the page, instead of being silently truncated into page text.
pub fn strip(text: &str) -> String {
    if find_block(text).is_none() && !text.lines().any(is_transcript_line) {
        return text.to_string();
    }
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some((at, open, close)) = find_block(rest) else {
            kept.push_str(rest);
            break;
        };
        kept.push_str(&rest[..at]);
        let after = &rest[at + open.len()..];
        rest = match after.find(close) {
            Some(end) => &after[end + close.len()..],
            None => after.split_once('\n').map_or("", |(_, tail)| tail),
        };
    }
    kept.lines()
        .filter(|l| !is_transcript_line(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// What one model answer stands for: the calls to run, and the prose around
/// them.
///
/// Structured `tool_calls[]` always win. Without them the answer is read as text
/// dialect and parsed the same way — whether the caller is still offering tools
/// is its own decision, not the parser's: a model that asks for a file after the
/// tool budget is spent asked for it either way, and dropping the request is
/// what leaves a page without text. `specs` only types the arguments; `tag`
/// namespaces the generated ids, which must stay unique across the whole
/// conversation a caller accumulates.
pub(crate) fn resolve(
    text: &str,
    calls: Vec<ToolCall>,
    specs: &[ToolSpec],
    tag: &str,
) -> (Vec<ToolCall>, String) {
    if !calls.is_empty() {
        return (calls, text.to_string());
    }
    match extract(text, specs, tag) {
        Some(d) => (d.calls, d.prose),
        None => (Vec::new(), strip(text)),
    }
}

struct Dialect {
    calls: Vec<ToolCall>,
    prose: String,
}

/// The calls a text dialect stands for, plus the prose around them. `None` when
/// the text holds no call this model could run.
fn extract(text: &str, specs: &[ToolSpec], tag: &str) -> Option<Dialect> {
    let calls: Vec<ToolCall> = function_blocks(text)
        .into_iter()
        .enumerate()
        .filter(|(_, (_, name, _))| !name.is_empty())
        .map(|(i, (_, name, body))| ToolCall {
            // Only has to match the `role: tool` message that answers it.
            id: format!("text_call_{tag}_{i}"),
            arguments: parameters(&name, body, specs).to_string(),
            name,
        })
        .collect();
    (!calls.is_empty()).then(|| Dialect {
        calls,
        prose: strip(text),
    })
}

/// Closed `<function=NAME>…</function>` blocks: offset, name, body. A block the
/// model never closed is not parsed — guessing where it ends would run the wrong
/// call.
fn function_blocks(text: &str) -> Vec<(usize, String, &str)> {
    const OPEN: &str = "<function=";
    const CLOSE: &str = "</function>";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(OPEN) {
        let at = from + at;
        let after = &text[at + OPEN.len()..];
        let Some(gt) = after.find('>') else { break };
        let body_start = gt + 1;
        let Some(end) = after[body_start..].find(CLOSE) else {
            break;
        };
        out.push((
            at,
            after[..gt].trim().trim_matches('"').to_string(),
            &after[body_start..body_start + end],
        ));
        from = at + OPEN.len() + body_start + end + CLOSE.len();
    }
    out
}

/// The `<parameter=…>` bodies of one function block, as the JSON arguments
/// object a tool call expects.
fn parameters(tool: &str, body: &str, specs: &[ToolSpec]) -> Value {
    let mut map = Map::new();
    let mut rest = body;
    while let Some(at) = rest.find("<parameter") {
        let after = &rest[at + "<parameter".len()..];
        let Some((key, after_name)) = parameter_key(after) else {
            break;
        };
        let Some(end) = after_name.find("</parameter>") else {
            break;
        };
        let value = after_name[..end].trim();
        let kind = declared_type(specs, tool, &key);
        map.insert(key, coerce(value, kind));
        rest = &after_name[end + "</parameter>".len()..];
    }
    Value::Object(map)
}

/// `<parameter=path>` / `<parameter name="path">` → the key and what follows its
/// closing `>`.
fn parameter_key(after: &str) -> Option<(String, &str)> {
    let rest = after.trim_start();
    let rest = rest.strip_prefix('=').or_else(|| {
        rest.strip_prefix("name")
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('='))
    })?;
    let rest = rest.trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let gt = rest.find('>')?;
    let key = rest[..gt].trim_end_matches(['"', '\'']).trim();
    Some((key.to_string(), &rest[gt + 1..]))
}

/// The JSON type the tool schema declares for a parameter, when it declares one.
fn declared_type<'a>(specs: &'a [ToolSpec], tool: &str, param: &str) -> Option<&'a str> {
    specs
        .iter()
        .find(|s| s.name == tool)?
        .parameters
        .get("properties")?
        .get(param)?
        .get("type")?
        .as_str()
}

/// Numbers stay numbers (`<parameter=start_line>121</parameter>` must reach the
/// tool as `121`), everything else stays a string — a path that happens to look
/// numeric is still a path.
fn coerce(value: &str, kind: Option<&str>) -> Value {
    match kind {
        Some("integer" | "number" | "boolean" | "array" | "object") => {
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
        }
        _ => Value::String(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn specs() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "read_file".into(),
            description: String::new(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer" },
                    "end_line": { "type": "integer" }
                },
                "required": ["path"]
            }),
        }]
    }

    fn args(call: &ToolCall) -> Value {
        serde_json::from_str(&call.arguments).expect("arguments are a JSON object")
    }

    /// 线上事故的原样：`<tool_call>` 里一个 read_file 带路径与行号区间。
    #[test]
    fn resolves_a_read_file_transcript() {
        let text = "正文。\n\n<tool_call>\n<function=read_file>\n<parameter=path>\ncrates/atlas-store/src/schema.rs\n</parameter>\n<parameter=start_line>\n121\n</parameter>\n<parameter=end_line>\n159\n</parameter>\n</function>\n</tool_call>\n";
        let (calls, prose) = resolve(text, Vec::new(), &specs(), "t");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(args(&calls[0])["path"], "crates/atlas-store/src/schema.rs");
        assert!(
            args(&calls[0])["start_line"].is_number(),
            "{:?}",
            args(&calls[0])
        );
        assert_eq!(prose, "正文。");
    }

    /// 一轮里多个调用（模型一次列好几个文件），全部还原。
    #[test]
    fn resolves_every_call_in_a_round() {
        let text = "<tool_call><function=read_file><parameter=path>a.rs</parameter></function></tool_call>\n<tool_call><function=read_file><parameter=path>b.rs</parameter></function></tool_call>\n";
        let (calls, prose) = resolve(text, Vec::new(), &specs(), "t");
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(args(&calls[0])["path"], "a.rs");
        assert_eq!(args(&calls[1])["path"], "b.rs");
        assert!(prose.is_empty(), "{prose}");
        assert_ne!(calls[0].id, calls[1].id);
    }

    /// 结构化 `tool_calls[]` 存在时优先，文本原样交给调用方。
    #[test]
    fn structured_calls_win() {
        let structured = vec![ToolCall {
            id: "call_1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
        }];
        let (calls, text) = resolve("正文", structured, &specs(), "t");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(text, "正文");
    }

    /// 工具预算用尽、schemas 不再随请求下发时，模型照样会这样要文件：
    /// 解析不看「是否提供工具」，执行与否由调用方按自己的预算决定。
    #[test]
    fn parses_calls_the_caller_still_has_to_decide_about() {
        let text = "前言\n<tool_call>\n<function=read_file>\n<parameter=path>a.rs\n</parameter>\n</function>\n</tool_call>\n";
        let (calls, prose) = resolve(text, Vec::new(), &specs(), "t");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(prose, "前言");
    }

    /// 没有可执行的调用时，转录必须摘干净，不能当正文交出去。
    #[test]
    fn strip_only_result_keeps_no_transcript() {
        let text = "前言\n<tool_call>\n<function=read_file>\n<parameter=path>a.rs\n</parameter>\n</function>\n</tool_call>\n";
        let stripped = strip(text);
        assert_eq!(stripped, "前言");
    }

    #[test]
    fn strip_leaves_clean_prose_alone() {
        let clean = "## 职责\n\n`crates/core/src/lib.rs:1` 定义入口。\n";
        assert_eq!(strip(clean), clean);
    }

    /// 未闭合的块只删自己那一行：残留的 `<parameter=…>` 必须继续可见，
    /// 让页面质量门判定「含工具调用残片」而不是静默截断。
    #[test]
    fn unclosed_block_keeps_its_leftovers_visible() {
        let text = "正文开头。\n\n<tool_call>\n<function=read_file>\n<parameter=path>\ncrates/x.rs\n</parameter>\n";
        let stripped = strip(text);
        assert!(stripped.contains("正文开头。"), "{stripped}");
        assert!(!stripped.contains("<function"), "{stripped}");
        assert!(stripped.contains("<parameter=path>"), "{stripped}");
    }

    /// 参数类型按工具 schema 还原：声明成 string 的保持字符串，整数保持数字。
    #[test]
    fn coerces_values_by_declared_type() {
        let text = "<function=read_file><parameter=path>121</parameter><parameter=start_line>121</parameter></function>";
        let (calls, _) = resolve(text, Vec::new(), &specs(), "t");
        let args = args(&calls[0]);
        assert_eq!(args["path"], "121");
        assert_eq!(args["start_line"], 121);
    }

    /// `<parameter name="x">` 是同一方言的另一种写法，键名同样取对。
    #[test]
    fn accepts_the_name_attribute_form() {
        let text = "<function=read_file><parameter name=\"path\">a.rs</parameter><parameter name='start_line'>7</parameter></function>";
        let (calls, _) = resolve(text, Vec::new(), &specs(), "t");
        let args = args(&calls[0]);
        assert_eq!(args["path"], "a.rs");
        assert_eq!(args["start_line"], 7);
    }

    /// 没有闭合标记的调用不还原：猜错的参数会被真的执行。
    #[test]
    fn ignores_unclosed_functions() {
        let text = "正文开头。\n\n<function=read_file>\n<parameter=path>\na.rs\n</parameter>\n";
        let (calls, prose) = resolve(text, Vec::new(), &specs(), "t");
        assert!(calls.is_empty());
        assert!(prose.contains("正文开头。"), "{prose}");
        assert!(!prose.contains("<function"), "{prose}");
        // 未闭合块留下的参数行继续可见，由上层门禁判定「含工具调用残片」。
        assert!(prose.contains("<parameter=path>"), "{prose}");
    }

    /// Anthropic / Kimi 的 `antml:` 变体不还原成调用，但整块要摘干净。
    #[test]
    fn strips_antml_invoke_blocks() {
        let text = "正文\nantml:invoke name=\"read_file\"\n<parameter=path>\na.rs\n</parameter>\n</antml:invoke>\n";
        let (calls, prose) = resolve(text, Vec::new(), &specs(), "t");
        assert!(calls.is_empty());
        assert_eq!(prose, "正文");
    }
}
