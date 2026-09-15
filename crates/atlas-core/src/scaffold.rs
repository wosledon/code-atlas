use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn write_example_config(repo_root: &Path) -> Result<PathBuf> {
    let p = repo_root.join("atlas.toml.example");
    let example = r#"# Code Atlas — 非密钥配置示例。密钥请用环境变量。
# OPENAI_API_KEY / ANTHROPIC_API_KEY / ATLAS_PROVIDER / ATLAS_MODEL

[llm]
provider = "openai-compatible"
model = "qwen2.5-coder:32b"
base_url = "http://127.0.0.1:11434/v1"
temperature = 0.2
max_output_tokens = 16384
concurrency = 5
timeout_secs = 180
retries = 3
# How many read_file/list_files/grep rounds a page generation may use (0 disables tools).
max_tool_rounds = 6

[privacy]
redact_paths = ["**/.env", "**/*.pem", "**/id_rsa*", "**/credentials*.json"]
max_file_bytes = 262144

[graph]
llm_extract = true
max_neighborhood_depth = 3
authoritative_min_confidence = 0.7

[kb]
search = "fts5"
embed_model = ""
max_context_bytes = 24576

[kb.chunk]
mode = "hybrid"
target_tokens = 512
chunk_model = ""

[output]
strategy = "in-repo"
atlas_root = "atlas"
external_root = ""
language = "zh-CN"

[analyze]
languages = ["rust", "typescript", "javascript", "python"]
"#;
    std::fs::write(&p, example)?;
    Ok(p)
}

pub fn ensure_agents_pointer(repo_root: &Path, atlas_root_rel: &str) -> Result<()> {
    let path = repo_root.join("AGENTS.md");
    let block_start = "<!-- ATLAS:START -->";
    let block_end = "<!-- ATLAS:END -->";
    let block = format!(
        "{block_start}\n## Atlas\n\n生成文档 Wiki 位于 `{atlas_root_rel}/`（入口 `{atlas_root_rel}/quickstart.md`）。\n\n- 检索：`atlas search \"...\"`\n- 更新：`atlas update`\n- 本地 UI：`atlas serve`\n{block_end}\n"
    );
    if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        if let (Some(s), Some(e)) = (text.find(block_start), text.find(block_end)) {
            let e2 = e + block_end.len();
            let mut new_text = String::new();
            new_text.push_str(&text[..s]);
            new_text.push_str(&block);
            new_text.push_str(&text[e2..]);
            std::fs::write(&path, new_text)?;
            return Ok(());
        }
        let mut new_text = text;
        if !new_text.ends_with('\n') {
            new_text.push('\n');
        }
        new_text.push('\n');
        new_text.push_str(&block);
        std::fs::write(&path, new_text)?;
    } else {
        std::fs::write(&path, block)?;
    }
    Ok(())
}
