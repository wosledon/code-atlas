//! 每一页的写作大纲与生成后的「深度体检」。
//!
//! 质量的第一道闸门是提示词：[`page_brief`] 把「这一页必须回答什么、深到什么程度」
//! 写成硬要求注入 prompt；第二道闸门是生成之后的 [`depth_gaps`]，用可判定的结构
//! 指标（篇幅、章节数、表格/图、真实代码引用、占位词）找出偷懒的页面，交给
//! `generate` 触发一次扩写。

use super::*;

/// 所有页面共用的深度门槛，原样进入提示词。
/// 指标对齐 wiki 里已生成的优质页（Business 入口表、Module 的 symbol:line API 表、
/// Onboarding 的真实命令表），并禁止工具残片与空洞表格。
const DEPTH_BAR: &str = r#"Hard quality bar (a page that misses any of these is a failed page, not a short one):
- Substance: >= 1200 characters, >= 4 `##` sections, every section carries real content (no one-line sections).
- Traceability: >= 8 backticked repository-relative paths or real symbol names; prefer `path:line` anchors you actually read with the tools. Never invent line numbers.
- Tables: at least one markdown table with real anchors; prefer >= 5 data rows when the repo has that many items. Empty cells or "见代码" are failures.
- Diagrams must PARSE. If you write a mermaid flowchart/sequence: node ids are plain ASCII identifiers (`[A-Za-z0-9_]`), never a mermaid keyword — `graph`, `end`, `subgraph`, `class`, `classDef`, `style`, `click`, `linkStyle`, `direction`, `default` — because those are lexed as syntax; wrap EVERY node and edge label in double quotes, e.g. `core["crates/atlas-core<br/>run_init_or_update"]`, `api -->|"POST /api/ask"| kb`; use `<br/>` (never a literal newline) for line breaks. Always quote; an unquoted label is a bug.
- Type-specific must-haves: Module pages include 职责 vs 非职责, public API table, dependency edges, 易错点, 上手要点. Business pages include 领域术语表 and 业务规则表 with file:function anchors. Workflow pages include stage I/O table and failure paths.
- Explain WHY, not only WHAT: intent, invariants, trade-offs, pitfalls, and what breaks if someone changes it.
- No placeholders ("需人工补全", "待补充", "TBD", "略", "见代码"): if something cannot be determined, say what you inspected and what stays unknown.
- End factual pages with `## Claims`: 5–8 short verifiable bullets, preferably each with a code anchor.
- Generic advice that would fit any repository is worthless: every paragraph must mention something that is true only for THIS repository."#;

const PLACEHOLDERS: &[&str] = &[
    "需人工补全",
    "由人工补充",
    "人工补充",
    "待补充",
    "待完善",
    "此处省略",
    "省略若干",
    "（略）",
    "TBD",
    "见代码",
    "（待确认）",
    "待确认",
];

/// 章节大纲：按页面类型要求必须出现的小节。每行一节，写成编号清单而不是
/// 用 `/` 串成一行——提示词里一行一个 `##` 标题，模型才会逐节写，也不会
/// 把整串标题当成一个小节名。
fn outline(page_type: &str) -> &'static str {
    match page_type {
        "Quickstart" => "\
1. ## 一句话定位
2. ## 它解决什么问题（谁在什么场景用）
3. ## 跑起来（真实命令 + 期望现象）
4. ## 运行时结构与入口（表格：入口 → 文件 → 作用）
5. ## 模块地图（表格：模块 → 路径 → 职责）
6. ## 推荐阅读顺序（表格：页面 → 你会得到什么）
7. ## 常见坑与注意事项",
        "Onboarding" => "\
1. ## 我需要哪些前提（工具链/版本/依赖服务）
2. ## 构建·测试·运行命令（表格，来自真实 manifest）
3. ## 代码地图（表格：目录 → 职责 → 代表文件）
4. ## 主调用链（mermaid flowchart，从入口到落盘）
5. ## 第一次改动怎么做（具体到文件与函数）
6. ## 调试与日志（怎么看一次运行发生了什么）",
        "Business" => "\
1. ## 业务问题（这个仓库替谁解决什么）
2. ## 使用者与使用场景（含真实入口：CLI 子命令 / HTTP 路由）
3. ## 领域术语表（表格：术语 → 含义 → 代码里的类型或文件）
4. ## 核心业务规则落在哪里（表格：规则 → 文件 → 关键函数）
5. ## 边界与非目标（代码里没有做的事）
6. ## 规则变更时的连锁影响",
        "Architecture" => "\
1. ## 分层与模块边界（每层职责与依赖方向）
2. ## 模块清单（表格：模块 → 路径 → 职责 → 对外入口）
3. ## 依赖关系（mermaid flowchart / graph LR，来自真实 import 与 manifest）
4. ## 关键运行时拓扑（进程、端口、存储）
5. ## 数据与状态如何流动（mermaid sequenceDiagram）
6. ## 扩展点与不可破坏的约束",
        "Data Model" => "\
1. ## 领域实体（表格：实体 → 代码定义 → 存储位置）
2. ## 持久化形态（文件 / 表 / schema，含真实字段）
3. ## 实体关系（mermaid erDiagram）
4. ## 读写路径与索引（谁写、谁读、怎么查找）
5. ## 迁移与初始化（首次如何建库/建索引，如何重建）
6. ## 兼容性与演进注意",
        "API Reference" => "\
1. ## 对外接口总表（表格：CLI 命令 / HTTP 路由 / 导出函数 → 参数 → 返回）
2. ## 入参与校验（错误与状态码）
3. ## 返回值与错误语义
4. ## 调用示例（真实命令或 curl，含期望输出）
5. ## 版本与兼容策略
6. ## 常见误用",
        "Workflow" => "\
1. ## 触发入口（命令 / 请求 / 事件）
2. ## 分阶段流程（mermaid sequenceDiagram 或 flowchart，逐阶段说明）
3. ## 每个阶段的输入·输出·副作用（表格）
4. ## 失败与回退路径（哪些失败会降级、降级成什么）
5. ## 并发与性能特征（并发度、限流、缓存）
6. ## 如何调试这条流程（日志、落盘产物、复现命令）",
        "Runbook" => "\
1. ## 健康检查怎么看（命令 + 正常/异常现象）
2. ## 常见故障 → 定位 → 修复（表格，至少 5 条）
3. ## 日志与观测点（文件、字段）
4. ## 改一个功能的步骤（列出涉及文件与顺序）
5. ## 数据重建与回滚
6. ## 风险清单与前置检查",
        "Module" => "\
1. ## 职责与非职责
2. ## 关键文件（表格：文件 → 作用 → 入口符号（带行号））
3. ## 对外 API（签名 + 定义位置）
4. ## 依赖关系（入边/出边，来自 manifest 与真实 use/import）
5. ## 数据与状态（读写什么、落在哪里）
6. ## 易错点与不变量
7. ## 上手要点（先读哪几个文件、第一个改动建议怎么做）",
        _ => "\
1. ## 背景与范围
2. ## 关键事实（表格：事实 → 代码位置）
3. ## 与代码的对应关系
4. ## 风险与注意事项",
    }
}

/// 根据页面类型给出最少篇幅（字符数）。
fn min_chars(page_type: &str) -> usize {
    match page_type {
        "Quickstart" | "Onboarding" => 1000,
        _ => 1200,
    }
}

/// 需要图示的页面类型（结构 / 流程 / 数据关系）。
fn wants_diagram(page_type: &str) -> bool {
    matches!(page_type, "Architecture" | "Data Model" | "Workflow" | "Module")
}

/// 注入提示词的完整写作要求：通用门槛 + 本页大纲 + 本页意图。
pub(crate) fn page_brief(page: &PlannedPage) -> String {
    let mut out = String::new();
    out.push_str(DEPTH_BAR);
    out.push_str("\n\nThis page\n");
    out.push_str(&format!("- section type: {}\n", page.page_type));
    out.push_str(&format!("- intent: {}\n", page.description));
    if let Some(module) = &page.module {
        out.push_str(&format!("- module key: {module}\n"));
    }
    if !page.tags.is_empty() {
        out.push_str(&format!("- tags: {}\n", page.tags.join(", ")));
    }
    out.push_str(
        "\nRequired sections for this page — emit exactly these `##` headings, in this order, \
         without renaming, merging or skipping any of them:\n",
    );
    out.push_str(outline(&page.page_type));
    out
}

/// 生成后的深度体检：返回未达标的项，空表示通过。
pub(crate) fn depth_gaps(body: &str, page: &PlannedPage) -> Vec<String> {
    let mut gaps = Vec::new();
    let chars = body.chars().count();
    let min = min_chars(&page.page_type);
    if chars < min {
        gaps.push(format!("正文只有 {chars} 字符，需要 ≥ {min} 字符的实质内容"));
    }
    let min_h2 = if page.page_type == "Module" { 5 } else { 4 };
    let h2 = body
        .lines()
        .filter(|l| l.trim_start().starts_with("## "))
        .count();
    if h2 < min_h2 {
        gaps.push(format!("只有 {h2} 个二级标题，需要 ≥ {min_h2} 个且每节有实质内容"));
    }
    let cites = code_citations(body);
    if cites < 8 {
        gaps.push(format!(
            "只引用了 {cites} 处真实代码路径/符号（反引号标注），需要 ≥ 8 处，尽量带 `path:line`（未读到的文件不要编行号）"
        ));
    }
    if let Some(ph) = PLACEHOLDERS.iter().find(|p| body.contains(**p)) {
        gaps.push(format!("出现占位表述「{ph}」，必须替换为查证后的结论"));
    }
    if let Some(marker) = super::quality::body_pollution(body) {
        gaps.push(format!(
            "正文含工具调用残片或生成标记「{marker}」：必须删除 transcript，只保留 wiki 正文"
        ));
    }
    // Type-specific bars aligned with the best existing wiki pages.
    match page.page_type.as_str() {
        "Module" => {
            if !body.contains("职责") || !body.contains("非职责") {
                gaps.push("Module 页必须同时包含「职责」与「非职责」".into());
            }
            if !body.contains("上手要点") && !body.contains("先读") {
                gaps.push("Module 页必须有上手要点（先读哪些文件、第一个改动建议）".into());
            }
        }
        "Business" => {
            if !body.contains("术语") && !body.contains("Glossary") {
                gaps.push("Business 页需要领域术语表（术语 → 含义 → 代码映射）".into());
            }
        }
        "Workflow" => {
            if !body.contains("```mermaid") {
                gaps.push("Workflow 页必须包含分阶段 mermaid 图".into());
            }
        }
        _ => {}
    }
    if count_lines_starting_with(body, '|') < 3 {
        gaps.push("没有足够的 markdown 表格：文件/符号/命令/接口类信息必须用带锚点的表格".into());
    }
    if wants_diagram(&page.page_type) && !body.contains("```mermaid") {
        gaps.push("缺少 mermaid 图：结构、流程或数据关系需要画出来".into());
    }
    if !body
        .lines()
        .any(|l| l.trim_start().starts_with('#') && l.contains("Claims"))
    {
        gaps.push("结尾缺少 `## Claims` 事实清单（5–8 条可核验 bullet）".into());
    } else {
        let claims_bullets = body
            .lines()
            .skip_while(|l| !(l.trim_start().starts_with('#') && l.contains("Claims")))
            .skip(1)
            .filter(|l| l.trim_start().starts_with("- "))
            .count();
        if claims_bullets < 5 {
            gaps.push(format!(
                "Claims 只有 {claims_bullets} 条，需要 5–8 条可核验事实"
            ));
        }
    }
    gaps
}

fn count_lines_starting_with(body: &str, ch: char) -> usize {
    body.lines()
        .filter(|l| l.trim_start().starts_with(ch))
        .count()
}

/// 统计反引号代码片段中看起来像真实路径/符号的引用数量。
fn code_citations(body: &str) -> usize {
    body.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| looks_like_code(span))
        .count()
}

fn looks_like_code(span: &str) -> bool {
    let span = span.trim();
    if span.len() < 4 || span.chars().any(char::is_whitespace) {
        return false;
    }
    let has_letter = span.chars().any(|c| c.is_alphabetic());
    let path_like = has_letter
        && (span.contains('/') || span.contains("::") || span.contains('.') || span.contains('('));
    let ident_like = span
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        && (span.contains('_') || span.chars().any(|c| c.is_uppercase()));
    path_like || ident_like
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(page_type: &str) -> PlannedPage {
        PlannedPage {
            rel_path: "03-架构/架构总览.md".into(),
            title: "架构总览".into(),
            page_type: page_type.into(),
            description: "分层、模块边界与依赖方向".into(),
            tags: vec!["architecture".into()],
            module: None,
            focus: "architecture overview".into(),
        }
    }

    #[test]
    fn brief_carries_type_specific_outline() {
        let arch = page_brief(&page("Architecture"));
        assert!(arch.contains("mermaid"));
        assert!(arch.contains("分层与模块边界"));
        assert!(arch.contains("Hard quality bar"));
        // 不同页面类型拿到不同大纲
        assert_ne!(arch, page_brief(&page("Runbook")));
    }

    /// 图必须能被 mermaid 解析：提示词里要写死 id 与标签的引号规则。
    #[test]
    fn brief_demands_parsable_mermaid() {
        let brief = page_brief(&page("Architecture"));
        assert!(brief.contains("Diagrams must PARSE"), "{brief}");
        assert!(brief.contains("wrap EVERY node and edge label in double quotes"), "{brief}");
        assert!(brief.contains("never a mermaid keyword"), "{brief}");
    }

    #[test]
    fn thin_page_is_flagged() {
        let gaps = depth_gaps("## 只有一节\n太短了，没有表格也没有图。", &page("Module"));
        assert!(gaps.iter().any(|g| g.contains("字符")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.contains("二级标题")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.contains("表格")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.contains("mermaid")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.contains("Claims")), "{gaps:?}");
    }

    #[test]
    fn placeholder_is_flagged() {
        let body = format!("{}\n需人工补全。", deep_body());
        let gaps = depth_gaps(&body, &page("Architecture"));
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(gaps[0].contains("待补充") || gaps[0].contains("人工"), "{gaps:?}");
    }

    #[test]
    fn deep_page_passes() {
        assert!(depth_gaps(&deep_body(), &page("Architecture")).is_empty());
    }

    /// 一页满足全部硬指标的内容，用于回归「深度门不要误判」。
    fn deep_body() -> String {
        let filler = "这一节解释了模块边界与依赖方向，为什么这样分层，以及改动后会破坏什么不变量。";
        let mut body = String::from("## 分层与模块边界\n");
        body.push_str(filler);
        body.push_str("\n\n## 模块清单\n| 模块 | 路径 | 职责 | 入口 |\n| --- | --- | --- | --- |\n| core | `crates/atlas-core` | 管线 | `run_init_or_update` |\n| server | `crates/atlas-server` | API | `serve` |\n| cli | `crates/atlas-cli` | 入口 | `main` |\n| store | `crates/atlas-store` | SQLite | `Store::open` |\n| kb | `crates/atlas-kb` | 检索 | `store_chunks` |\n\n");
        body.push_str("## 依赖关系\n```mermaid\ngraph LR\n  cli[\"crates/atlas-cli\"] --> core[\"crates/atlas-core\"]\n```\n");
        body.push_str("## 扩展点与约束\n");
        for _ in 0..24 {
            body.push_str(filler);
        }
        body.push_str(
            "\n\n引用 `crates/atlas-core/src/pipeline/run.rs:42`、`crates/atlas-core/src/config/mod.rs:15`、\
             `crates/atlas-server/src/lib.rs`、`crates/atlas-cli/src/main.rs`、`web/src/App.tsx`、\
             `atlas.toml`、`scan_repo()`、`run_init_or_update`、`Store::open` 与 `build_evidence`。\n",
        );
        body.push_str("\n## Claims\n");
        body.push_str("- 分层为 crates 边界，见 `crates/atlas-core`。\n");
        body.push_str("- 流水线入口是 `run_init_or_update`。\n");
        body.push_str("- 配置加载在 `AtlasConfig::load`。\n");
        body.push_str("- 存储由 `Store::open` 打开。\n");
        body.push_str("- HTTP 服务入口是 `atlas-server::serve`。\n");
        body
    }
}
