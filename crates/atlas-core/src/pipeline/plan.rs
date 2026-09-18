use super::plan_modules::detect_code_modules;
use super::*;

#[derive(Debug, Clone)]
pub struct PlannedPage {
    pub rel_path: String,
    pub title: String,
    pub page_type: String,
    pub description: String,
    pub tags: Vec<String>,
    pub module: Option<String>,
    pub focus: String,
}

/// Preview documentation tree for any repo without calling LLM.
pub fn preview_plan(repo_root: &Path, instruction: Option<&str>) -> Result<Vec<PlannedPage>> {
    let scan = scan_repo(repo_root)?;
    Ok(plan_pages(&scan, "plan", instruction))
}

/// 文档树面向「接手代码库」：业务/设计/模块/数据/接口/流程/运维
pub(crate) fn plan_pages(scan: &RepoScan, mode: &str, instruction: Option<&str>) -> Vec<PlannedPage> {
    let mut pages: Vec<PlannedPage> = Vec::new();
    #[allow(clippy::too_many_arguments)]
    fn add(
        pages: &mut Vec<PlannedPage>,
        rel: &str,
        title: &str,
        ptype: &str,
        desc: &str,
        tags: &[&str],
        module: Option<String>,
        focus: String,
    ) {
        pages.push(PlannedPage {
            rel_path: rel.into(),
            title: title.into(),
            page_type: ptype.into(),
            description: desc.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            module,
            focus,
        });
    }

    add(
        &mut pages,
        "quickstart.md",
        "快速上手",
        "Quickstart",
        "5 分钟看懂这个仓库：它解决什么问题、怎么跑起来、按什么顺序读下去",
        &["nav", "quickstart"],
        None,
        "quickstart for THIS repository: what it is, the problem it solves, how to install/build/run \
         it, the entry points, and the recommended reading order for a maintainer. Ground every step \
         in real files and commands found in the repo."
            .into(),
    );

    // 00 上手
    add(
        &mut pages,
        "00-快速上手/第一天上手.md",
        "第一天上手",
        "Onboarding",
        "环境、构建、跑起来、改哪里最安全",
        &["onboarding"],
        None,
        "first-day guide for a new developer: setup, build, run, safest first change".into(),
    );
    add(
        &mut pages,
        "00-快速上手/代码地图.md",
        "代码地图",
        "Onboarding",
        "目录职责与入口文件索引",
        &["onboarding"],
        None,
        "code map: every top-level directory and entry file with one-line responsibility".into(),
    );

    // 01 业务
    add(
        &mut pages,
        "01-业务领域/业务与领域.md",
        "业务与领域",
        "Business",
        "解决什么问题、用户场景、领域概念",
        &["business"],
        None,
        "business domain: problem, users, scenarios, domain language grounded in code".into(),
    );
    add(
        &mut pages,
        "01-业务领域/核心概念.md",
        "核心概念",
        "Business",
        "领域对象与术语如何落到代码",
        &["business"],
        None,
        "map domain concepts to code types/modules".into(),
    );

    // 02 设计
    add(
        &mut pages,
        "02-系统设计/整体架构.md",
        "整体架构",
        "Architecture",
        "分层、依赖、运行时拓扑（mermaid）",
        &["design"],
        None,
        "architecture for maintainers: layers, dependencies, runtime topology with mermaid".into(),
    );
    add(
        &mut pages,
        "02-系统设计/关键设计决策.md",
        "关键设计决策",
        "Architecture",
        "为什么这样拆、边界与取舍",
        &["design"],
        None,
        "key design decisions and tradeoffs visible in the code".into(),
    );
    add(
        &mut pages,
        "02-系统设计/数据流与状态.md",
        "数据流与状态",
        "Architecture",
        "主数据如何流动、状态落哪里（mermaid）",
        &["design"],
        None,
        "data flow and state ownership with mermaid diagrams".into(),
    );

    // 03 模块：按任意仓库结构动态发现（文件名保留代码包名，目录中文）
    let modules = detect_code_modules(scan);
    for (key, label, path_hint) in &modules {
        let slug = key.replace(['/', '\\', ' ', ':'], "-");
        pages.push(PlannedPage {
            rel_path: format!("03-模块详解/{slug}.md"),
            title: label.clone(),
            page_type: "Module".into(),
            description: format!("{label}（`{path_hint}`）：职责、关键文件、对外 API、依赖"),
            tags: vec!["module".into(), key.clone()],
            module: Some(key.clone()),
            focus: format!(
                "explain module `{label}` under path `{path_hint}` for a maintainer: \
                 responsibility, key source files with real paths, public entrypoints/API, \
                 inbound/outbound dependencies, common pitfalls. Ground only in scanned sources."
            ),
        });
    }

    // 04 数据：通用，不绑定具体 crate 名
    add(
        &mut pages,
        "04-数据模型/数据模型.md",
        "数据模型",
        "Data Model",
        "领域实体、存储形态、关系（mermaid/erDiagram）",
        &["data"],
        None,
        "data model discovered from this repo: entities, DB tables/schema files, ORMs, \
         message payloads — include mermaid erDiagram or class diagram when evidence exists".into(),
    );
    add(
        &mut pages,
        "04-数据模型/持久化与一致性.md",
        "持久化与一致性",
        "Data Model",
        "写哪里、谁是权威、如何重建/迁移",
        &["data"],
        None,
        "persistence and consistency: source of truth, caches/indexes, migrations, rebuild paths".into(),
    );

    // 05 接口
    add(
        &mut pages,
        "05-接口说明/对外接口.md",
        "对外接口",
        "API Reference",
        "CLI / HTTP / RPC / SDK 等稳定对外面",
        &["api"],
        None,
        "external interfaces of THIS repo: CLI commands, HTTP routes, RPC, exports — \
         tables of method/path/params grounded in code".into(),
    );
    add(
        &mut pages,
        "05-接口说明/配置面.md",
        "配置面",
        "API Reference",
        "配置项、环境变量、开关如何影响行为",
        &["api", "config"],
        None,
        "configuration surface: config files, env vars, feature flags and their behavioral impact".into(),
    );

    // 06 关键流程
    add(
        &mut pages,
        "06-关键流程/流程-生成文档.md",
        "流程：生成文档",
        "Workflow",
        "init/update 端到端（sequence）",
        &["flow"],
        None,
        "end-to-end wiki generation flow with sequence diagram".into(),
    );
    add(
        &mut pages,
        "06-关键流程/流程-问答检索.md",
        "流程：问答检索",
        "Workflow",
        "chat 如何用库与模型回答",
        &["flow"],
        None,
        "end-to-end chat/retrieval flow with fallback".into(),
    );

    // 07 接手
    add(
        &mut pages,
        "07-接手运维/接手手册.md",
        "接手手册",
        "Runbook",
        "常见故障、日志、怎么改一处功能",
        &["ops", "takeover"],
        None,
        "takeover runbook: failures, logs, how to change one feature safely".into(),
    );
    add(
        &mut pages,
        "07-接手运维/已知边界与债.md",
        "已知边界与债",
        "Runbook",
        "MVP 裁剪、TODO、风险点",
        &["ops"],
        None,
        "known limitations, tech debt, risks for the next maintainer".into(),
    );

    if let Some(instr) = instruction {
        let instr = instr.trim();
        if !instr.is_empty() {
            // Run-level instruction applies to every page (focus feeds prompts
            // and page_fingerprint, so a new instruction regenerates the wiki).
            for p in &mut pages {
                p.focus = format!(
                    "{}\n\nRun-level instruction for this generation (apply when it touches this page): {instr}",
                    p.focus
                );
            }
            pages.push(PlannedPage {
                rel_path: "08-专项说明/专项说明.md".into(),
                title: "专项说明".into(),
                page_type: "Workflow".into(),
                description: instr.chars().take(120).collect(),
                tags: vec!["focus".into()],
                module: None,
                focus: format!(
                    "Special-topic page driven by the run instruction. Expand it into a full \
                     workflow page grounded in this repository.\n\nRun instruction: {instr}"
                ),
            });
        }
    }
    let _ = mode;
    pages.truncate(30);
    pages
}
