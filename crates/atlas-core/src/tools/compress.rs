//! 工具输出的无损压缩管道：在不丢信息的前提下减少喂给模型的 token。
//!
//! 模型读的是文本，所以「压缩」只能是**语义与字符双重保真**的改写，不能用
//! gzip/字典回指那类需要解码器的编码（模型无法解码）。这里四种变换各自成立：
//!
//! | 变换 | 去掉什么 | 可逆性 |
//! | --- | --- | --- |
//! | [`trim_line_ends`] | CRLF、行尾空白、重复的空格分隔 | 除行尾空白外逐字节相同 |
//! | [`collapse_blank_runs`] | 连续 ≥3 个空行只留 1 个 | 空行个数不是信息（正文语义不变） |
//! | [`hoist_common_indent`] | 整块公共缩进（头部声明空格数） | **精确可逆**：按声明的列数缩回 |
//! | [`fold_identical_runs`] | 连续 ≥4 行完全相同（保留 1 行 + 计数） | **精确可逆**：按计数展开 |
//!
//! 所有变换都只作用于**工具返回给模型**的文本（源码窗口、grep、目录树），
//! 不触碰仓库里的文件本身；`read_file` 的行号栏仍按原始行号生成，所以引用
//! `path:line` 不受影响。

/// 一次变换的结果：压缩后文本 + 省下的字符数（用于汇总统计）。
pub(crate) struct Compacted {
    pub text: String,
    pub removed: usize,
}

impl Compacted {
    fn of(before: usize, text: String) -> Self {
        let removed = before.saturating_sub(text.len());
        Self { text, removed }
    }
}

/// 连续这么多行完全相同才折叠：3 行以内折叠收益抵不上标记的开销。
const FOLD_MIN_RUN: usize = 4;
/// 公共缩进小于这个宽度就不外提：省下的字符还不够写那行头部。
const HOIST_MIN_INDENT: usize = 4;
/// 外提缩进的块至少要这么多行，否则头部成本摊不平。
const HOIST_MIN_LINES: usize = 8;

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

/// 3. 公共缩进外提：整块共有的前导空格只在头部声明一次，正文按声明缩回即可复原。
///
/// 返回 `(缩进列数, 压缩后的文本)`；列数为 0 表示这块不值得外提。
pub(crate) fn hoist_common_indent(text: &str) -> (usize, Compacted) {
    let lines: Vec<&str> = text.split('\n').collect();
    let indents: Vec<usize> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .collect();
    let common = indents.iter().copied().min().unwrap_or(0);
    if common < HOIST_MIN_INDENT || indents.len() < HOIST_MIN_LINES {
        return (0, Compacted::of(text.len(), text.to_string()));
    }
    let before = text.len();
    let mut out = format!("// （以下整块缩进 {common} 空格，已在每行省略）\n");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(&line[common.min(line.len())..]);
    }
    (common, Compacted::of(before, out))
}

/// 4. 重复行折叠：连续 ≥[`FOLD_MIN_RUN`] 行完全相同 → 保留首行并标注 `⟪同上 ×N⟫`。
///
/// 输入是 `(行号, 内容)`；折叠只影响内容，行号保留首行的，所以被折掉的那些行
/// 的内容与首行逐字相同，`path:line` 引用不会因此失真。返回省下的字符数。
pub(crate) fn fold_identical_runs(lines: Vec<(usize, String)>) -> (Vec<(usize, String)>, usize) {
    let mut out: Vec<(usize, String)> = Vec::with_capacity(lines.len());
    let mut removed = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let (no, ref content) = lines[i];
        let mut run = 1usize;
        while i + run < lines.len() && lines[i + run].1 == *content {
            run += 1;
        }
        if run >= FOLD_MIN_RUN {
            // 空行/纯缩进行折了没意义，反而让读者以为正文被删了。
            if content.trim().is_empty() {
                for item in lines[i..i + run].iter() {
                    out.push(item.clone());
                }
            } else {
                removed += content.len() * (run - 1);
                out.push((no, format!("{content}  ⟪同上 ×{run}⟫")));
            }
        } else {
            for item in lines[i..i + run].iter() {
                out.push(item.clone());
            }
        }
        i += run;
    }
    (out, removed)
}

/// 自由文本上的完整管道（1+2+3），供非 `read_file` 的工具输出使用。
pub(crate) fn compact(text: &str) -> Compacted {
    let trimmed = trim_line_ends(text);
    let collapsed = collapse_blank_runs(&trimmed.text);
    let (_, hoisted) = hoist_common_indent(&collapsed.text);
    Compacted {
        text: hoisted.text,
        removed: trimmed.removed + collapsed.removed + hoisted.removed,
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

    /// 公共缩进外提必须精确可逆：按头部声明的列数缩回就能逐字节复原。
    #[test]
    fn hoisted_indent_is_exactly_reversible() {
        let body: String = (1..=12)
            .map(|i| format!("        let v{i} = {i};\n"))
            .collect();
        let (cols, out) = hoist_common_indent(&body);
        assert_eq!(cols, 8, "{}", out.text);
        assert!(out.removed > 0, "{}", out.text);
        assert!(
            out.text.starts_with("// （以下整块缩进 8 空格"),
            "{}",
            out.text
        );

        let restored: String = out
            .text
            .lines()
            .skip(1)
            .map(|l| format!("{}{}\n", " ".repeat(cols), l))
            .collect();
        assert_eq!(restored, body, "外提必须可逆");
    }

    /// 缩进太浅或行数太少就不外提：省下的不如头部声明贵。
    #[test]
    fn shallow_or_short_blocks_are_left_alone() {
        let shallow = "  let a = 1;\n  let b = 2;\n";
        assert_eq!(hoist_common_indent(shallow).0, 0);

        let short = "        one\n        two\n";
        assert_eq!(hoist_common_indent(short).0, 0);
    }

    /// 重复行折叠：4 行以上才折叠，计数写进标记，按标记能精确展开。
    #[test]
    fn folds_identical_runs_reversibly() {
        let mut lines: Vec<(usize, String)> = Vec::new();
        for i in 1..=3 {
            lines.push((i, format!("line {i}")));
        }
        for i in 4..=9 {
            lines.push((i, "    }".to_string()));
        }
        lines.push((10, "tail".to_string()));

        let (folded, removed) = fold_identical_runs(lines);
        assert!(removed > 0);
        // 1..3 原样；4..9 折成一行并标注；10 原样。
        assert_eq!(folded.len(), 5, "{folded:?}");
        assert_eq!(folded[3].0, 4, "折叠行保留首行行号");
        assert_eq!(folded[3].1, "    }  ⟪同上 ×6⟫", "计数就是被折掉的行数");
        assert_eq!(folded[4], (10, "tail".to_string()));
        // 精确可逆：按计数展开回 6 行，内容与原文逐行一致。
        let base = folded[3].1.split("  ⟪").next().unwrap().to_string();
        assert_eq!(base, "    }");
        let expanded: Vec<String> = std::iter::repeat_n(base, 6).collect();
        assert_eq!(expanded.len(), 6);
        assert!(expanded.iter().all(|l| l == "    }"));
    }

    /// 空行不折叠：否则读者会以为正文被删掉了。
    #[test]
    fn blank_runs_are_never_folded() {
        let lines: Vec<(usize, String)> = (1..=6).map(|i| (i, String::new())).collect();
        let (out, removed) = fold_identical_runs(lines);
        assert_eq!(out.len(), 6);
        assert_eq!(removed, 0);
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
