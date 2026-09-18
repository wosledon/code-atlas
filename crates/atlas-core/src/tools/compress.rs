//! 工具输出的无损压缩管道：在不丢信息的前提下减少喂给模型的 token。
//!
//! 模型读的是文本，所以「压缩」只能是**语义与字符双重保真**的改写，不能用
//! gzip/字典回指那类需要解码器的编码（模型无法解码）。四种变换各自成立：
//!
//! | 变换 | 去掉什么 | 用在 | 可逆性 |
//! | --- | --- | --- | --- |
//! | [`trim_line_ends`] | CRLF、行尾空白 | `grep` / 目录树等自由文本 | 除行尾空白外逐字节相同 |
//! | [`collapse_blank_runs`] | 连续 ≥3 个空行只留 1 个 | 同上 | 空行个数不是信息 |
//! | [`indent_blocks`] | 整块公共缩进（块首尾各一行记号） | **仅 `read_file`** | **精确可逆** |
//! | [`fold_identical_runs`] | 连续 ≥4 个完全相同的行 | **仅 `read_file`** | 保留首行 + `(xN)` |
//!
//! 是**两条**管道而不是一条：`read_file` 每一行都带原始行号（`path:line` 引用靠它），
//! 所以不能折叠空行——行号会错位——走的是「逐行 clip → 缩进外提 → 重复行折叠」；
//! 其余工具输出是自由文本，走 [`compact`]（行尾归一 + 空行折叠）。
//!
//! **按块**而不是按窗口外提缩进：实测（本仓 85 万字符的 rs/ts/tsx/json）整窗外提
//! 只省 2.6%，因为真实读窗口常从第 1 行（0 缩进）开始；改成按最小缩进成块后省 6.2%。
//! 记号成对出现（`// begin: N spaces omitted per line` 开始、`// end` 结束），读者把
//! 块内每行补上 N 个空格就能**逐字节复原**；记号行不占行号，所以源码行号仍是原始值。
//!
//! 缩进外提是这里唯一「语言相关」的算法，闸门有三道（见 [`hoist_allowed`]）：
//! ① 只有 `HOIST_LANGUAGES` 白名单里的扩展名才允许外提，不在表里的一律跳过——缩进
//! 即语法的语言（Python / YAML / Markdown / SASS / Pug …）不在表里，所以永不外提；
//! ② 跨行字符串里行首空格是数据的行，由 [`string_lines`] 逐行标出、块会绕开它们
//! （Go 反引号原生字符串、JS/TS 模板字符串、Java/C# 三引号）；③ 净收益低于
//! `MIN_BLOCK_GAIN` 的块不压，免得"省下的不够写记号"。
//!
//! [`fold_identical_runs`] 在**手写代码里几乎不触发**（本仓 101 个 `.rs` + 36 个
//! `.ts/.tsx` 实测连续 ≥4 行完全相同的情况一次都没出现），但在**生成代码**里很值：
//! protobuf 生成物里连续 8 行的 `_ = fileDescriptor` 会折叠成一行 `(x8)`。所以它是
//! "有则省、无则不动"的兜底项，不是主力——主力是按块缩进外提与行号栏收窄（每行 7
//! 字符 ≈ 全仓 20% 的字符，已由 `read_file` 收窄到实际宽度）。

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
/// 也不计入行数），直到遇到缩进更浅的行、或 `blocked` 标记的行为止（后者是跨行
/// 字符串，见 [`string_lines`]）；块内最小缩进就是可省的前导空格数。嵌套更深的行
/// 属于外层块，相对深度因此完整保留。
///
/// 为什么按「最小缩进」而不是按「等缩进段」分块：实测本仓 85 万字符的 rs/ts/tsx/json，
/// 等缩进段只能省 0.9%——真实代码的每一层往往只有几行，达不到 [`MIN_BLOCK_LINES`]；
/// 按最小缩进成块能省 6.2%，因为一段函数体（含其中的嵌套块）是一条长块。
pub(crate) fn indent_blocks(lines: &[&str], blocked: &[bool]) -> Vec<IndentBlock> {
    let is_blocked = |i: usize| blocked.get(i).copied().unwrap_or(false);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if !is_hoistable_line(lines[i]) || is_blocked(i) {
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
            // 跨行字符串内的行是内容，不是代码：它终止本块。
            if is_blocked(j) || leading_spaces(line) < MIN_INDENT {
                break;
            }
            indent = indent.min(leading_spaces(line));
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

/// 允许外提缩进的扩展名（**显式白名单**：不在这张表里的一律不压）。
///
/// 判据是「缩进只影响可读性、不影响语义」。反例——Python / YAML / Markdown / reST /
/// SASS / Pug / Haskell / F# / Nim——不在这张表里，所以永远不会被外提。
/// 用白名单而不是黑名单：漏压只是少省几个字符，误压会把写进页面的代码改错。
// 表按类分组手工维护；`rustfmt` 会把分组注释拽到上一行行尾，反而把「kts/scala/…」
// 读成 Web 语言，所以这张表跳过格式化。
#[rustfmt::skip]
const HOIST_LANGUAGES: &[&str] = &[
    // 系统语言
    "rs", "go", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx", "m", "mm", "java", "cs", "kt",
    "kts", "scala", "swift", "dart", "zig",
    // Web / 脚本
    "js", "jsx", "mjs", "cjs", "ts", "tsx", "svelte", "css", "scss", "less", "sh", "bash",
    "zsh", "ps1", "bat", "cmd",
    // schema / 数据（空白无语义）
    "json", "jsonc", "proto", "graphql", "gql", "sql",
    // 构建清单
    "gradle", "lock", "sum", "mod",
];

/// 反引号是跨行字符串定界符的语言：反引号之间的行首空格是**字符串内容**。
const BACKTICK_STRINGS: &[&str] = &[
    "go", "js", "jsx", "mjs", "cjs", "ts", "tsx", "svelte", "sh", "bash", "zsh",
];

/// 三引号是跨行字符串定界符的语言（含 C# 11 的原始字符串、Java 的文本块）。
const TRIPLE_QUOTE_STRINGS: &[&str] = &["java", "cs", "kt", "kts", "scala", "dart", "swift"];

/// 能否对这段文本做缩进外提。
///
/// 这是**整窗**级别的门禁（语言白名单 + C# 逐字字符串）；更细的「这一行是不是在
/// 跨行字符串里」由 [`string_lines`] 逐行判定，`indent_blocks` 会绕开那些行。
pub(crate) fn hoist_allowed(ext: &str, text: &str) -> bool {
    if !HOIST_LANGUAGES.contains(&ext) {
        return false;
    }
    // C# 逐字字符串 `@"…"` 可跨行且保留原样空白；逐字符跟踪收益不抵复杂度，
    // 出现就整窗跳过（这类文件仍然享受空白归一与空行折叠）。
    !(ext == "cs" && text.contains("@\""))
}

/// 标记窗口里**不能动**的行：处于跨行字符串内部、或开启/结束一个跨行字符串的行。
///
/// 这些行的行首空格是字符串内容（Go 的原生字符串里常见 SQL/HTML 模板，JS 的模板
/// 字符串里常见缩进过的片段），外提会把内容改错。按「本行定界符数量为奇数就在本行
/// 翻转状态」判定——不需要词法分析器，代价是行内出现奇数个定界符的罕见写法会被
/// 保守地跳过（少省几个字符，不会改错）。
pub(crate) fn string_lines(lines: &[&str], ext: &str) -> Vec<bool> {
    let mut flags = vec![false; lines.len()];
    let backtick = BACKTICK_STRINGS.contains(&ext);
    let triple = TRIPLE_QUOTE_STRINGS.contains(&ext);
    if !backtick && !triple {
        return flags;
    }
    let (mut in_backtick, mut in_triple) = (false, false);
    for (i, line) in lines.iter().enumerate() {
        let started = in_backtick || in_triple;
        if backtick && line.matches('`').count() % 2 == 1 {
            in_backtick = !in_backtick;
        }
        // 三引号要减去反引号里的三连标记？不必：这两类字符串不会互相嵌套。
        if triple && line.matches("\"\"\"").count() % 2 == 1 {
            in_triple = !in_triple;
        }
        // 跨越字符串的行、以及开启它的那一行，都标记为不可动。
        flags[i] = started || in_backtick || in_triple;
    }
    flags
}

/// 连续 ≥[`FOLD_MIN_RUN`] 行**渲染后完全相同** → 保留首行并标注 `  (xN)`。
///
/// 这类重复多出现在生成代码与数据里（protobuf 生成的 `_ = fileDescriptor` 连续块、
/// schema 里成片的同形条目），手写代码里几乎见不到——所以它是**有则省、无则不动**的
/// 兜底项，而不是主力（主力是按块缩进外提与行号栏收窄）。
///
/// 输入是 `(原始行号, 渲染文本)`；折叠只丢重复行的冗余副本，**保留的行仍用原始行号**，
/// 所以 `path:line` 引用不会失效——行号会出现跳号，正是「这里被折了 N 行」的信号。
/// 返回 `(折叠后的行, 省下的字符数)`。
pub(crate) fn fold_identical_runs(lines: Vec<(usize, String)>) -> (Vec<(usize, String)>, usize) {
    let mut out: Vec<(usize, String)> = Vec::with_capacity(lines.len());
    let mut saved = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let (no, ref text) = lines[i];
        let mut run = 1usize;
        while i + run < lines.len() && lines[i + run].1 == *text {
            run += 1;
        }
        if run >= FOLD_MIN_RUN && !text.trim().is_empty() {
            let marker = format!("  (x{run})");
            // 标记挂在保留行上；省下的是 (run-1) 行内容 + 换行，再扣掉标记。
            let gain = (run - 1) * (text.chars().count() + 1) - marker.chars().count();
            if gain > 0 {
                saved += gain;
                out.push((no, format!("{text}{marker}")));
                i += run;
                continue;
            }
        }
        for item in lines[i..i + run].iter() {
            out.push(item.clone());
        }
        i += run;
    }
    (out, saved)
}

/// 折叠判定用的最小连续行数：3 行以内，标记的成本抵不过省下的内容。
pub(crate) const FOLD_MIN_RUN: usize = 4;

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

    /// 测试助手：这些文本里没有跨行字符串，没有任何行需要绕开。
    fn no_strings(lines: &[&str]) -> Vec<bool> {
        vec![false; lines.len()]
    }

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

    /// 按块外提的证明：把 `// begin: N spaces omitted per line` 到 `// end` 之间的
    /// 行各补回 N 个空格，逐字节复原原文。
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

        let blocks = indent_blocks(&refs, &no_strings(&refs));
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
        assert!(
            indent_blocks(&shallow, &no_strings(&shallow)).is_empty(),
            "缩进 2 空格不值得"
        );

        let short: Vec<&str> = (1..=4).map(|_| "        one();").collect();
        assert!(
            indent_blocks(&short, &no_strings(&short)).is_empty(),
            "只有 4 行不值得"
        );

        // 外层 4 空格 + 内层 12 空格：外层不够本（4×11 − 记号 < 2×记号），
        // 算法往内挪一行后按内层 12 空格成块——内层相对深度同样保留。
        let mut nested: Vec<String> = vec!["    outer();".into()];
        nested.extend((0..10).map(|_| "            inner();".to_string()));
        let refs: Vec<&str> = nested.iter().map(String::as_str).collect();
        let blocks = indent_blocks(&refs, &no_strings(&refs));
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].indent, 12, "取深块的缩进");
        assert_eq!(blocks[0].start, 1, "浅前缀被排除在块外");
        assert_eq!(strip_indent(refs[5], 12), "inner();");
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

        let blocks = indent_blocks(&refs, &no_strings(&refs));
        let saved: usize = blocks.iter().map(|b| b.saved()).sum();
        assert!(!blocks.is_empty(), "长块必须被找到：{blocks:?}");
        assert_eq!(blocks[0].indent, 4, "外层块按最小缩进成块");
        assert!(
            saved * 100 >= window_chars * 5,
            "省下 {saved} / {window_chars} 字符，低于 5% 底线"
        );
    }

    /// 问题清单里的七种语言都必须在白名单里（缩进在它们里都无语义）。
    #[test]
    fn common_brace_languages_are_hoisted() {
        for ext in [
            "c", "h", "cc", "cpp", "cxx", "hpp", "cs", "go", "rs", "java", "proto",
        ] {
            assert!(hoist_allowed(ext, "fn f() {}"), "{ext} 应当允许外提缩进");
        }
    }

    /// 白名单是显式的：表外的扩展名（含缩进即语法的语言与未知语言）一律不压。
    #[test]
    fn languages_outside_the_allow_list_are_never_hoisted() {
        for ext in [
            "py", "yml", "yaml", "md", "rst", "sass", "pug", "hs", "fs", "nim", "txt", "csv",
            "toml", "ini", "html", "xml", "vue", "rb", "php", "lua", "", "unknown",
        ] {
            assert!(
                !hoist_allowed(ext, "        indented = true"),
                "{ext} 不该外提缩进"
            );
        }
        // C# 的逐字字符串允许换行且保留原样空白：出现就整窗跳过。
        assert!(hoist_allowed("cs", "var s = \"x\";"));
        assert!(!hoist_allowed("cs", "var s = @\"line1\n    line2\";"));
    }

    /// Go 原生字符串（反引号）里的行首空格是字符串内容：块必须绕开它。
    #[test]
    fn multiline_strings_are_excluded_from_blocks() {
        let lines = [
            "func query() string {",
            "    sql := `SELECT *",
            "FROM t",
            "WHERE id = 1`",
            "    if sql == \"\" {",
            "        return \"\"",
            "    }",
            "    return sql",
            "}",
        ];
        let flags = string_lines(&lines, "go");
        assert!(
            flags[1] && flags[2] && flags[3],
            "字符串那几行要标记：{flags:?}"
        );
        assert!(!flags[4], "字符串之后的代码行可以动：{flags:?}");

        // 没有跨行字符串时，同样的形状可以成块外提。
        let plain: Vec<String> = (0..12)
            .map(|i| format!("        let v{i} = {i};"))
            .collect();
        let refs: Vec<&str> = plain.iter().map(String::as_str).collect();
        assert!(
            string_lines(&refs, "rs").iter().all(|f| !*f),
            "纯代码没有要绕开的行"
        );
        assert_eq!(indent_blocks(&refs, &no_strings(&refs)).len(), 1);

        // 含原生字符串的 Go 片段：块被字符串切断，不会把 SQL 的缩进抹掉。
        let blocked = string_lines(&lines, "go");
        for b in indent_blocks(&lines, &blocked) {
            assert!(b.start > 3 || b.end <= 1, "块不能覆盖跨行字符串：{b:?}");
        }
    }

    /// JS 模板字符串同理；单行模板字符串不影响。
    #[test]
    fn template_literals_are_excluded_too() {
        let spanned = ["const a = `", "    kept", "`;"];
        let flags = string_lines(&spanned, "ts");
        assert!(
            flags.iter().all(|f| *f),
            "跨行模板字符串的每一行都要标记：{flags:?}"
        );

        let inline = ["const a = `x`;", "const b = `y`;", "const c = `z`;"];
        let flags = string_lines(&inline, "ts");
        assert!(
            flags.iter().all(|f| !*f),
            "行内闭合的模板字符串不影响：{flags:?}"
        );
    }

    /// 生成代码里成片的相同行会被折叠，且保留行号、可逆。
    #[test]
    fn generated_repetition_is_folded_reversibly() {
        let mut rows: Vec<(usize, String)> = vec![(1, "var (".into())];
        for i in 2..=7 {
            rows.push((i, "    _ = fileDescriptor".into()));
        }
        rows.push((8, ")".into()));

        let (folded, saved) = fold_identical_runs(rows);
        assert!(saved > 0, "六行相同应当有净收益");
        assert_eq!(folded.len(), 3, "{folded:?}");
        assert_eq!(folded[1].0, 2, "保留首行的原始行号");
        assert_eq!(folded[1].1, "    _ = fileDescriptor  (x6)");
        assert_eq!(
            folded[2],
            (8, ")".into()),
            "下一行行号不变（跳号=折了 5 行）"
        );

        // 可逆：按计数展开回 6 行。
        let base = folded[1].1.split("  (x").next().unwrap().to_string();
        let expanded: Vec<String> = std::iter::repeat_n(base, 6).collect();
        assert_eq!(expanded.len(), 6);
        assert!(expanded.iter().all(|l| l == "    _ = fileDescriptor"));
    }

    /// 手写代码里两三个相同行很常见：达不到 4 行就不折，免得标记比省下的还贵。
    #[test]
    fn short_runs_are_left_alone() {
        let rows: Vec<(usize, String)> = (1..=3).map(|i| (i, "    }".into())).collect();
        let (out, saved) = fold_identical_runs(rows);
        assert_eq!(out.len(), 3);
        assert_eq!(saved, 0);

        // 空行不折：读者会以为内容被删了（空行折叠已单独处理连续空行）。
        let blanks: Vec<(usize, String)> = (1..=8).map(|i| (i, String::new())).collect();
        let (out, saved) = fold_identical_runs(blanks);
        assert_eq!(out.len(), 8);
        assert_eq!(saved, 0);
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
