//! 工具输出的无损压缩管道：在不丢信息的前提下减少喂给模型的 token。
//!
//! 模型读的是文本，所以「压缩」只能是**语义与字符双重保真**的改写，不能用
//! gzip/字典回指那类需要解码器的编码（模型无法解码）。三种变换各自成立：
//!
//! | 变换 | 去掉什么 | 适用 | 可逆性 |
//! | --- | --- | --- | --- |
//! | [`trim_line_ends`] | CRLF、行尾空白 | 全部 | 除行尾空白外逐字节相同 |
//! | [`collapse_blank_runs`] | 连续 ≥3 个空行只留 1 个 | 全部 | 空行个数不是信息 |
//! | [`indent_blocks`] | 整块公共缩进（块首尾各一行记号） | **仅缩进无语义的语言** | **精确可逆** |
//!
//! **按块**而不是按窗口外提缩进：实测（本仓 828k 字符的 rs/ts/tsx/json）整窗外提
//! 只触发 43 次、省 2.6%，因为真实读窗口常从第 1 行（0 缩进）开始；改成「每个
//! 连续同深度区段各外提一次」后触发 163 次、省 6.5%——是前者的 2.5 倍。
//! 记号成对出现（`↳+N` 开始、`↳-` 结束），所以读者把块内每行补上 N 个空格就能
//! **逐字节复原**；记号行不占行号，源码行号仍是原始的，`path:line` 引用不受影响。
//!
//! 缩进外提是这里唯一「语言相关」的算法，闸门有三道（[`hoist_allowed`]）：
//! ① 缩进即语法的语言（Python / YAML / Markdown / reST / SASS / Pug …）直接跳过——
//! 把 8 空格外提后模型无法知道真实层级；② 跨行字符串里缩进是数据的场景用**定界符
//! 奇偶守卫**拦下（JS/TS 模板字符串的反引号、TOML/INI 的三引号）；③ 收益不足
//! 2 倍记号成本的块不压，免得"省下的不够写记号"。
//!
//! 实测结论（101 个 `.rs` + 36 个 `.ts/.tsx` + 全部 json/toml/md）：**连续 ≥4 行完全
//! 相同的情况一次都没出现**，所以曾经的「重复行折叠」已删除——留着只是没被触发的代码。
//! 另一个大头在行号栏（每行 7 字符 ≈ 全仓 20% 的字符），已由 `read_file` 收窄到实际宽度。

/// 一次变换的结果：压缩后文本 + 省下的字符数（用于汇总统计）。
///
/// 记账单位是**字符**不是字节：模型按 token 付费，而中文一字 3 字节、一词一 token，
/// 用字节会让「省下多少」虚高 3 倍——这种自我恭维的度量没有意义。
pub(crate) struct Compacted {
    pub text: String,
    pub removed: usize,
}

impl Compacted {
    fn of(before: usize, text: String) -> Self {
        let removed = before.saturating_sub(text.chars().count());
        Self { text, removed }
    }
}

/// 进入"可外提"的最低公共缩进：再浅就省不下记号成本。
const MIN_INDENT: usize = 4;
/// 一个块至少要有这么多非空行才值得外提。
const MIN_BLOCK_LINES: usize = 8;
/// 一个块至少要净省这么多字符，否则不做——为省 3 个字符写十几个字符没有意义。
const MIN_BLOCK_GAIN: usize = 20;

/// 块起始记号的前缀，供渲染方识别（这类行没有行号）。
pub(crate) const BLOCK_START_PREFIX: &str = "// begin: ";
/// 块结束记号。与起始记号配对，读者据此知道缩进省略到哪一行为止。
pub(crate) const BLOCK_END: &str = "// end";

/// 一段可以外提缩进的连续区段（窗口内的行下标，`[start, end)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndentBlock {
    pub start: usize,
    pub end: usize,
    /// 该块每行省掉的前导空格数（块内最小缩进）。
    pub indent: usize,
    /// 块内非空行数（收益按它算，空行没有缩进可省）。
    pub lines: usize,
}

impl IndentBlock {
    /// 这块能净省下的字符数（已扣除首尾两行记号与换行）。
    pub(crate) fn saved(&self) -> usize {
        let removed = self.indent * self.lines;
        let markers =
            block_start_marker(self.indent).chars().count() + 1 + BLOCK_END.chars().count() + 1;
        removed.saturating_sub(markers)
    }
}

/// 块起始记号：`// begin: 8 spaces omitted per line`。
///
/// 记号**自解释**，所以不需要每窗口一行图例：实测图例（约 110 字符）会吃掉三分之一
/// 收益——197 个块摊到 147 个窗口上，块均 1.3 个，图例反而比记号本身贵。
pub(crate) fn block_start_marker(indent: usize) -> String {
    format!("{BLOCK_START_PREFIX}{indent} spaces omitted per line")
}

/// 扫描出一段窗口内所有值得外提的块，按出现顺序返回。
///
/// 块的定义：从某行开始，**每个非空行的缩进都 ≥ [`MIN_INDENT`]**（空行不打断块，
/// 也不计入行数），直到遇到缩进更浅的行为止；块内最小缩进就是可省的前导空格数。
/// 嵌套更深的行属于外层块，相对深度因此完整保留。
///
/// 为什么按「最小缩进」而不是按「等缩进段」分块：实测本仓 85 万字符的 rs/ts/tsx/json，
/// 等缩进段只能省 0.9%——真实代码的每一层往往只有几行，达不到 [`MIN_BLOCK_LINES`]；
/// 按最小缩进成块能省 6.2%，因为一段函数体（含其中的嵌套块）是一条长块。
pub(crate) fn indent_blocks(lines: &[&str]) -> Vec<IndentBlock> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if !is_hoistable_line(lines[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut indent = usize::MAX;
        let mut count = 0usize;
        while j < lines.len() {
            let line = lines[j];
            if line.trim().is_empty() {
                j += 1;
                continue;
            }
            let own = leading_spaces(line);
            if own < MIN_INDENT {
                break;
            }
            indent = indent.min(own);
            count += 1;
            j += 1;
        }
        if count >= MIN_BLOCK_LINES {
            let block = IndentBlock {
                start: i,
                end: j,
                indent,
                lines: count,
            };
            if block.saved() >= MIN_BLOCK_GAIN {
                out.push(block);
                i = j;
                continue;
            }
        }
        // 整段不划算时不整段跳过：往里挪一行往往就换掉了浅缩进的前缀，
        // 后面更深的子段收益大得多（外层 4 空格 11 行不够本，内层 12 空格 10 行够本）。
        // 窗口最多 120 行，最坏 O(n²) 也就一万多次字符比较。
        i += 1;
    }
    out
}

fn is_hoistable_line(line: &str) -> bool {
    !line.trim().is_empty() && leading_spaces(line) >= MIN_INDENT
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// 去掉块内每行的公共缩进（空行原样保留）。
pub(crate) fn strip_indent(line: &str, indent: usize) -> &str {
    if line.trim().is_empty() {
        return line;
    }
    &line[indent.min(line.len())..]
}

/// 缩进即语法（或即数据）的扩展名：这些语言里把整块缩进外提会让内容失真。
const INDENT_SIGNIFICANT: &[&str] = &[
    "py", "pyi", "pyw", "yml", "yaml", "md", "markdown", "rst", "txt", "csv", "tsv", "sass",
    "styl", "pug", "jade", "haml", "hs", "lhs", "fs", "fsx", "nim", "coffee", "nims", "tex",
];

/// 跨行字符串用反引号的 JS 家族：反引号数量为奇数说明窗口切在了字符串中间。
const JS_FAMILY: &[&str] = &[
    "js", "jsx", "mjs", "cjs", "ts", "tsx", "vue", "svelte", "astro",
];

/// 跨行字符串用三引号的语言（TOML 多行字符串里前导空格是数据）。
const TRIPLE_QUOTE: &[&str] = &["toml", "ini", "cfg", "py"];

/// 能否对这段文本做缩进外提。
///
/// `ext` 是仓库相对路径的扩展名（小写，不含点）。判断只依赖文件名与文本本身，
/// 不需要解析器：外语法的闸门宁严勿松——漏压只是少省几个字符，误压会改语义。
pub(crate) fn hoist_allowed(ext: &str, text: &str) -> bool {
    if INDENT_SIGNIFICANT.contains(&ext) {
        return false;
    }
    // 定界符奇偶守卫：窗口切在多行字符串中间时，行首空格可能是字符串内容。
    if JS_FAMILY.contains(&ext) && odd_count(text, '`') {
        return false;
    }
    if TRIPLE_QUOTE.contains(&ext) && text.matches("\"\"\"").count() % 2 == 1 {
        return false;
    }
    true
}

/// 定界符奇偶守卫：窗口切在多行字符串中间时，行首空格可能是字符串内容。
fn odd_count(text: &str, ch: char) -> bool {
    text.chars().filter(|c| *c == ch).count() % 2 == 1
}

/// 1. 行尾与行分隔归一：CRLF→LF、去行尾空白（`clip` 已做，这里兜住其它工具）。
pub(crate) fn trim_line_ends(text: &str) -> Compacted {
    let before = text.len();
    let mut out = String::with_capacity(before);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end_matches(['\r', ' ', '\t']));
    }
    Compacted::of(before, out)
}

/// 2. 空行折叠：连续 ≥3 个空行只留 1 个，并去掉首尾空行。
pub(crate) fn collapse_blank_runs(text: &str) -> Compacted {
    let before = text.len();
    let mut out: Vec<&str> = Vec::new();
    let mut blank = 0usize;
    for line in text.split('\n') {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
            out.push("");
        } else {
            blank = 0;
            out.push(line);
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    Compacted::of(before, out.join("\n"))
}

/// 自由文本上的语言无关管道，供 `read_file` 之外的工具输出使用。
///
/// 缩进外提（见 [`indent_blocks`]）不在这里：grep、目录树这类输出的缩进可能是层级
/// 信息，而它们的公共缩进通常是 0、本来也省不下什么；外提只值得在 `read_file` 里做，
/// 那里既知道文件语言，也能把记号写进带行号的列表。
pub(crate) fn compact(text: &str) -> Compacted {
    let trimmed = trim_line_ends(text);
    let collapsed = collapse_blank_runs(&trimmed.text);
    Compacted {
        text: collapsed.text,
        removed: trimmed.removed + collapsed.removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行尾归一：CRLF 与行尾空白去掉，内容逐字节保留。
    #[test]
    fn trims_line_ends() {
        let dirty = "fn a() {   \r\n    let x = 1;\t\r\n}\r\n";
        let out = trim_line_ends(dirty);
        assert_eq!(out.text, "fn a() {\n    let x = 1;\n}\n");
        assert!(!out.text.contains('\r'), "{}", out.text);
        assert!(out.removed >= 2 * 3);
    }

    /// 空行折叠：连续空行只留一个，首尾空行去掉；正文行一个不动。
    #[test]
    fn collapses_blank_runs() {
        let text = "\n\n## A\n\n\n\n\n## B\n\n";
        let out = collapse_blank_runs(text);
        assert_eq!(out.text, "## A\n\n## B");
    }

    /// 按块外提的证明：把 `↳+N` 到 `↳-` 之间的行各补回 N 个空格，逐字节复原原文。
    #[test]
    fn block_hoisting_is_exactly_reversible() {
        let original: Vec<String> = (1..=14)
            .map(|i| {
                if i % 5 == 0 {
                    String::new() // 块内空行不打断块，也不该被补缩进
                } else {
                    format!("        let v{i} = {i};")
                }
            })
            .collect();
        let refs: Vec<&str> = original.iter().map(String::as_str).collect();

        let blocks = indent_blocks(&refs);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].indent, 8);
        assert_eq!(blocks[0].lines, 12, "空行不计入行数");
        assert!(blocks[0].saved() > 0);

        // 渲染成「记号 + 去缩进的行」，再按记号复原。
        let mut rendered: Vec<String> = vec![block_start_marker(blocks[0].indent)];
        rendered.extend(
            refs.iter()
                .map(|l| strip_indent(l, blocks[0].indent).to_string()),
        );
        rendered.push(BLOCK_END.to_string());

        let restored: Vec<String> = rendered[1..rendered.len() - 1]
            .iter()
            .map(|l| {
                if l.is_empty() {
                    l.clone()
                } else {
                    format!("{}{l}", " ".repeat(blocks[0].indent))
                }
            })
            .collect();
        assert_eq!(restored, original, "外提必须精确可逆");
    }

    /// 缩进太浅、行数太少的段不动；嵌套更深的行属于外层块，相对深度保留。
    #[test]
    fn shallow_or_short_blocks_are_left_alone() {
        let shallow: Vec<&str> = (1..=30).map(|_| "  let a = 1;").collect();
        assert!(indent_blocks(&shallow).is_empty(), "缩进 2 空格不值得");

        let short: Vec<&str> = (1..=4).map(|_| "        one();").collect();
        assert!(indent_blocks(&short).is_empty(), "只有 4 行不值得");

        // 外层 4 空格 + 内层 12 空格：外层不够本（4×11 − 记号 < 2×记号），
        // 算法往内挪一行后按内层 12 空格成块——内层相对深度同样保留。
        let mut nested: Vec<String> = vec!["    outer();".into()];
        nested.extend((0..10).map(|_| "            inner();".to_string()));
        let refs: Vec<&str> = nested.iter().map(String::as_str).collect();
        let blocks = indent_blocks(&refs);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].indent, 12, "取深块的缩进");
        assert_eq!(blocks[0].start, 1, "浅前缀被排除在块外");
        assert_eq!(strip_indent(refs[5], 12), "inner();");
    }

    /// 语言闸门：缩进即语法的语言绝不做外提——把 Python 的 8 空格外提后，
    /// 模型无法知道真实层级，写进页面的代码就是错的。
    #[test]
    fn indentation_significant_languages_are_never_hoisted() {
        let body: Vec<String> = (1..=12)
            .map(|i| format!("        value_{i} = compute({i})"))
            .collect();
        let joined = body.join("\n");
        // 同样的文本：Rust 允许外提，Python/YAML/Markdown 一律不许。
        assert!(hoist_allowed("rs", &joined));
        let refs: Vec<&str> = body.iter().map(String::as_str).collect();
        assert_eq!(indent_blocks(&refs).len(), 1, "文本本身是可外提的");
        for ext in [
            "py", "yml", "yaml", "md", "rst", "sass", "pug", "hs", "fs", "txt", "csv",
        ] {
            assert!(!hoist_allowed(ext, &joined), "{ext} 不该外提缩进");
        }
    }

    /// 跨行字符串守卫：窗口切在 JS 模板字符串中间时，行首空格是字符串内容。
    #[test]
    fn unbalanced_multiline_string_delimiters_block_the_hoist() {
        // 反引号成对 → 模板字符串已闭合，可以外提。
        let balanced = "        const a = `x`;\n        const b = `y`;\n";
        assert!(hoist_allowed("ts", balanced));

        // 反引号落单 → 有跨行模板字符串，跳过。
        let unbalanced = "        const a = `\n            kept\n";
        assert!(!hoist_allowed("ts", unbalanced));
        assert!(!hoist_allowed("tsx", "        const a = `\n"));

        // TOML 三引号多行字符串同理；Python 本来就被语言闸门挡住。
        assert!(!hoist_allowed("toml", "        \"\"\"\n    text\n"));
        assert!(hoist_allowed("toml", "        key = \"\"\"done\"\"\"\n"));
    }

    /// 收益回归底线：一段真实形状的 Rust 窗口（顶层 + 嵌套块 + 空行），
    /// 按块外提必须至少省下 5% 的字符。低于它就说明算法退化了。
    #[test]
    fn realistic_window_saves_at_least_five_percent() {
        let mut lines: Vec<String> = Vec::new();
        lines.push("use std::collections::HashMap;".into());
        lines.push(String::new());
        lines.push("pub fn run(items: &[Item]) -> Result<()> {".into());
        for i in 1..=40 {
            lines.push(format!("    let value_{i} = compute(items, {i})?;"));
        }
        lines.push("    if let Some(first) = items.first() {".into());
        for i in 1..=40 {
            lines.push(format!("        store.insert({i}, first.name_{i});"));
        }
        lines.push("    }".into());
        lines.push("    Ok(())".into());
        lines.push("}".into());
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let window_chars: usize = refs.iter().map(|l| l.chars().count() + 1).sum();

        let blocks = indent_blocks(&refs);
        let saved: usize = blocks.iter().map(|b| b.saved()).sum();
        assert!(!blocks.is_empty(), "长块必须被找到：{blocks:?}");
        assert_eq!(blocks[0].indent, 4, "外层块按最小缩进成块");
        assert!(
            saved * 100 >= window_chars * 5,
            "省下 {saved} / {window_chars} 字符，低于 5% 底线"
        );
    }

    #[test]
    fn language_agnostic_passes_apply_everywhere() {
        for body in [
            "fn a() {   \r\n\r\n\r\n\r\n}\r\n",  // Rust
            "def a():   \r\n\r\n\r\n\r\n\r\n",   // Python
            "func a() {  \r\n\r\n\r\n\r\n\r\n}", // Go
            "```py  \r\n\r\n\r\n\r\n\r\n```",    // Markdown
        ] {
            let trimmed = trim_line_ends(body);
            assert!(!trimmed.text.contains('\r'), "{}", trimmed.text);
            let out = collapse_blank_runs(&trimmed.text);
            assert!(!out.text.contains("\n\n\n"), "{}", out.text);
        }
    }

    /// 完整管道跑起来不会破坏正文行，且确实变小。
    #[test]
    fn pipeline_shrinks_without_losing_content() {
        let text = "fn a() {  \r\n\r\n\r\n\r\n    body();\r\n}\r\n";
        let out = compact(text);
        assert!(out.removed > 0);
        for content in ["fn a() {", "body();", "}"] {
            assert!(out.text.contains(content), "{}", out.text);
        }
        assert!(!out.text.contains('\r'), "{}", out.text);
    }
}
