//! 代码仓库静态扫描：目录遍历、忽略规则匹配、语言识别与符号提取。
//!
//! 模块划分：
//! - [`types`]：扫描结果的数据结构（[`RepoScan`]、[`SourceFile`]）。
//! - [`ignore`]：`.gitignore` / `.atlasignore` 规则解析与匹配。
//! - [`scan`]：仓库遍历、语言探测与统计。
//! - [`symbols`]：轻量符号提取与文件片段读取。
//! - [`outline`]：带行号的文件骨架（提示词用的 `path:line` 代码地图）。

mod ignore;
mod outline;
mod scan;
mod symbols;
mod types;

pub use ignore::IgnoreRules;
pub use outline::file_outline;
pub use scan::{detect_language, scan_repo, scan_repo_with_skips};
pub use symbols::{extract_symbols, read_file_excerpt};
pub use types::{OutlineItem, RepoScan, SourceFile};
