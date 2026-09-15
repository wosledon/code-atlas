use super::*;

use super::evidence::{entry_files, lang_of, module_scope, run_commands, scoped_files, todo_hotspots, top_dirs};
use super::plan::{plan_pages};

/// Deterministic fallback used when no LLM is configured (or the model returned
/// nothing usable). It is derived from the real scan so the wiki still explains
/// the repository structure instead of emitting placeholders.
pub(crate) fn template_page_body(scan: &RepoScan, page: &PlannedPage, cfg: &AtlasConfig) -> String {
    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", page.title));
    body.push_str(&format!(
        "> ⚠️ 模板模式：未检测到可用的 LLM，本页由结构扫描生成，仅覆盖可自动提取的事实。\
         配置 `[llm]` 后重新运行 `atlas update` 可获得完整讲解。\n\n"
    ));
    body.push_str(&format!(
        "- 仓库根：`{}`\n- 源文件：{} 个\n- 语言：{}\n- 输出语言：{}\n\n",
        scan.root.display(),
        scan.files.iter().filter(|f| f.language.is_some()).count(),
        if scan.languages.is_empty() {
            "unknown".to_string()
        } else {
            scan.languages.join(", ")
        },
        cfg.output.language
    ));

    let dirs = top_dirs(scan);
    let entries = entry_files(scan);

    match page.page_type.as_str() {
        "Quickstart" => {
            body.push_str("## 这个仓库是什么\n\n");
            if let Some(r) = &scan.readme_excerpt {
                let head: String = r.lines().take(12).collect::<Vec<_>>().join("\n");
                body.push_str(&head);
                body.push_str("\n\n");
            } else {
                body.push_str("未找到 README，请先补一份仓库说明。\n\n");
            }
            body.push_str("## 目录结构\n\n");
            body.push_str("| 目录 | 源文件 |\n| --- | --- |\n");
            for (d, n) in dirs.iter().take(15) {
                body.push_str(&format!("| `{d}` | {n} |\n"));
            }
            body.push_str("\n## 入口文件\n\n");
            for f in &entries {
                body.push_str(&format!("- `{}` · {} · {} B\n", f.rel, lang_of(f), f.size));
            }
            let cmds = run_commands(scan);
            if !cmds.is_empty() {
                body.push_str("\n## 常用命令\n\n```bash\n");
                for c in &cmds {
                    body.push_str(&format!("{c}\n"));
                }
                body.push_str("```\n");
            }
            body.push_str("\n## 阅读顺序\n\n");
            for p in plan_pages(scan, "plan", None) {
                body.push_str(&format!("- [{}]({}) — {}\n", p.title, p.rel_path, p.description));
            }
        }
        "Onboarding" => {
            body.push_str("## 代码地图\n\n");
            body.push_str("| 目录 | 源文件 | 职责（需人工补全） |\n| --- | --- | --- |\n");
            for (d, n) in dirs.iter().take(20) {
                body.push_str(&format!("| `{d}` | {n} |  |\n"));
            }
            body.push_str("\n## 入口文件\n\n");
            for f in &entries {
                body.push_str(&format!("- `{}` · {} · {} B\n", f.rel, lang_of(f), f.size));
            }
            body.push_str("\n## 建议第一处改动\n\n");
            body.push_str(&format!(
                "从 `{}` 开始跟踪一次完整调用链。\n",
                entries.first().map(|f| f.rel.as_str()).unwrap_or("(未识别到入口文件)")
            ));
            let cmds = run_commands(scan);
            if !cmds.is_empty() {
                body.push_str("\n## 构建与验证\n\n```bash\n");
                for c in cmds.iter().filter(|c| c.contains("test") || c.contains("build") || c.contains("clippy")) {
                    body.push_str(&format!("{c}\n"));
                }
                body.push_str("```\n");
            }
        }
        "Business" => {
            body.push_str("## 仓库自述\n\n");
            if let Some(r) = &scan.readme_excerpt {
                body.push_str(&r.chars().take(3000).collect::<String>());
                body.push_str("\n\n");
            }
            body.push_str("## 概念到代码的映射（按目录）\n\n");
            for (d, n) in dirs.iter().take(12) {
                body.push_str(&format!("- `{d}`（{n} 个源文件）\n"));
            }
        }
        "Architecture" => {
            body.push_str("## 顶层结构\n\n");
            body.push_str("| 目录 | 源文件 |\n| --- | --- |\n");
            for (d, n) in dirs.iter().take(15) {
                body.push_str(&format!("| `{d}` | {n} |\n"));
            }
            body.push_str("\n```mermaid\nflowchart TD\n");
            for (i, (d, _)) in dirs.iter().take(10).enumerate() {
                body.push_str(&format!("  D{i}[\"{d}\"]\n"));
            }
            if dirs.len() > 1 {
                for i in 1..dirs.len().min(10) {
                    body.push_str(&format!("  D0 -.-> D{i}\n"));
                }
            }
            body.push_str("```\n\n> 该图为按目录结构自动推导，真实依赖关系需配置 LLM 后重新生成。\n");
            body.push_str("\n## 清单文件\n\n");
            for m in scan.manifests.iter().take(15) {
                body.push_str(&format!("- `{}`\n", m.display()));
            }
        }
        "Module" => {
            let scope = module_scope(page);
            body.push_str(&format!(
                "## 范围\n\n`{}`\n\n",
                scope.as_deref().unwrap_or(page.module.as_deref().unwrap_or("(unknown)"))
            ));
            let files = scoped_files(scan, page);
            body.push_str(&format!("共 {} 个源文件。\n\n", files.len()));
            body.push_str("| 文件 | 语言 | 字节 |\n| --- | --- | --- |\n");
            for f in files.iter().take(60) {
                body.push_str(&format!("| `{}` | {} | {} |\n", f.rel, lang_of(f), f.size));
            }
            body.push_str("\n## 公开符号（启发式提取）\n\n");
            let mut count = 0;
            for f in files.iter().take(40) {
                let text = read_file_excerpt(&f.path, 120_000);
                for (kind, name) in extract_symbols(f, &text).into_iter().take(8) {
                    body.push_str(&format!("- `{kind}` `{name}` — `{}`\n", f.rel));
                    count += 1;
                }
                if count > 60 {
                    break;
                }
            }
            if count == 0 {
                body.push_str("- 未提取到符号。\n");
            }
        }
        "Data Model" => {
            body.push_str("## 检测到的数据相关文件\n\n");
            let hints = ["schema", "migration", "model", "entity", "db", "sql", "store", "repository"];
            for f in scan.files.iter().filter(|f| f.language.is_some()) {
                let l = f.rel.to_lowercase();
                if hints.iter().any(|h| l.contains(h)) {
                    body.push_str(&format!("- `{}` · {} · {} B\n", f.rel, lang_of(f), f.size));
                }
            }
            body.push_str("\n## 存储清单\n\n");
            for m in scan.manifests.iter().take(15) {
                body.push_str(&format!("- `{}`\n", m.display()));
            }
        }
        "API Reference" => {
            body.push_str("## 对外符号（启发式提取）\n\n");
            let mut count = 0;
            for f in scan.files.iter().filter(|f| f.language.is_some()) {
                let name = f.rel.rsplit('/').next().unwrap_or(&f.rel);
                if !(name == "lib.rs" || name == "main.rs" || name == "index.ts" || name == "mod.rs" || name == "App.tsx") {
                    continue;
                }
                let text = read_file_excerpt(&f.path, 120_000);
                for (kind, sym) in extract_symbols(f, &text).into_iter().take(10) {
                    body.push_str(&format!("- `{kind}` `{sym}` — `{}`\n", f.rel));
                    count += 1;
                }
                if count > 80 {
                    break;
                }
            }
            body.push_str("\n## 配置文件\n\n");
            for m in scan.manifests.iter().take(20) {
                body.push_str(&format!("- `{}`\n", m.display()));
            }
        }
        "Workflow" => {
            body.push_str("## 目录与文件线索\n\n");
            for (d, n) in dirs.iter().take(15) {
                body.push_str(&format!("- `{d}`（{n} 个源文件）\n"));
            }
            body.push_str("\n## CI/自动化\n\n");
            for f in scan.files.iter().filter(|f| f.rel.contains(".github/workflows") || f.rel.contains(".gitlab-ci")) {
                body.push_str(&format!("- `{}`\n", f.rel));
            }
        }
        _ => {
            body.push_str("## 已知技术债线索\n\n");
            let hits = todo_hotspots(scan);
            if hits.is_empty() {
                body.push_str("- 未在源文件中发现 TODO/FIXME/XXX 标记。\n");
            } else {
                body.push_str("| 文件 | 标记数 |\n| --- | --- |\n");
                for (f, n) in hits {
                    body.push_str(&format!("| `{f}` | {n} |\n"));
                }
            }
            body.push_str("\n## 构建与测试\n\n```bash\n");
            for c in run_commands(scan) {
                body.push_str(&format!("{c}\n"));
            }
            body.push_str("```\n");
        }
    }

    body.push_str("\n## Claims\n");
    body.push_str(&format!(
        "- 仓库 `{}` 含 {} 个源文件，语言 {}。\n",
        scan.root.display(),
        scan.files.iter().filter(|f| f.language.is_some()).count(),
        if scan.languages.is_empty() {
            "unknown".to_string()
        } else {
            scan.languages.join(", ")
        }
    ));
    body.push_str(&format!(
        "- 本页在模板模式下生成（focus: `{}`）。\n",
        page.focus.chars().take(120).collect::<String>()
    ));
    body
}
