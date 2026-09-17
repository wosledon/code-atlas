use anyhow::Result;
use std::path::{Path, PathBuf};

/// Default config template. Secrets may live in env vars or `[llm].api_key`.
pub(crate) fn config_template() -> &'static str {
    r#"# Code Atlas 配置。密钥可写在 [llm].api_key，或用环境变量（环境变量优先）。
# OPENAI_API_KEY / ANTHROPIC_API_KEY / ATLAS_PROVIDER / ATLAS_MODEL
# 此文件可提交；本地真实配置 atlas.toml / atlas.config.json 已在 .gitignore。

[llm]
provider = "openai-compatible"
model = "qwen2.5-coder:32b"
# 可选：直接写密钥。远程 OpenAI 兼容 / Anthropic 需要；本机 Ollama 可留空。
api_key = ""
base_url = "http://127.0.0.1:11434/v1"
temperature = 0.2
max_output_tokens = 16384
concurrency = 5
timeout_secs = 180
retries = 3
# How many read_file/list_files/grep rounds a page generation may use (0 disables tools).
max_tool_rounds = 6
# Re-check each page against the depth bar and let the model expand it once when too thin.
depth_pass = true

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
"#
}

/// Write `atlas.toml` (or another file name) from the default template.
pub fn write_config_file(repo_root: &Path, file_name: &str, force: bool) -> Result<PathBuf> {
    let p = repo_root.join(file_name);
    if p.exists() && !force {
        anyhow::bail!("{} already exists (use --force to overwrite)", p.display());
    }
    std::fs::write(&p, config_template())?;
    Ok(p)
}

pub fn write_example_config(repo_root: &Path) -> Result<PathBuf> {
    write_config_file(repo_root, "atlas.toml.example", true)
}

pub fn ensure_agents_pointer(repo_root: &Path, atlas_root_rel: &str) -> Result<()> {
    let path = repo_root.join("AGENTS.md");
    let block_start = "<!-- ATLAS:START -->";
    let block_end = "<!-- ATLAS:END -->";
    let block = format!(
        "{block_start}\n## Atlas\n\n生成文档 Wiki 位于 `{atlas_root_rel}/`（入口 `{atlas_root_rel}/quickstart.md`）。\n\n- 检索：`atlas search \"...\"`\n- 更新：`atlas update`\n- 本地 UI：`atlas web`\n{block_end}\n"
    );
    if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        if let (Some(s), Some(e)) = (text.find(block_start), text.find(block_end)) {
            // Consume the end-marker line's own terminator: otherwise the
            // newline that belongs to the existing block survives the
            // replacement and the file grows by one byte on every
            // `atlas update` — which changes the scan fingerprint of every
            // page and forces a full regeneration on the next run.
            let mut e2 = e + block_end.len();
            if text[e2..].starts_with("\r\n") {
                e2 += 2;
            } else if text[e2..].starts_with('\n') {
                e2 += 1;
            }
            let mut new_text = String::new();
            new_text.push_str(&text[..s]);
            new_text.push_str(&block);
            new_text.push_str(&text[e2..]);
            if new_text != text {
                std::fs::write(&path, new_text)?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "atlas-scaffold-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn agents_pointer_is_idempotent() {
        let root = temp_repo("agents");
        std::fs::write(root.join("AGENTS.md"), "# House rules\n\nBe nice.\n").unwrap();

        ensure_agents_pointer(&root, "atlas").unwrap();
        let first = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(first.contains("<!-- ATLAS:START -->"));
        assert!(first.starts_with("# House rules"));

        for _ in 0..5 {
            ensure_agents_pointer(&root, "atlas").unwrap();
            let again = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
            assert_eq!(first, again, "AGENTS.md must not change on repeated runs");
        }

        // Whitespace that already follows the block is left untouched.
        std::fs::write(root.join("AGENTS.md"), format!("{first}\n\n\n")).unwrap();
        let padded = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        ensure_agents_pointer(&root, "atlas").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            padded
        );

        // The block is rewritten in place when the wiki location changes.
        ensure_agents_pointer(&root, "docs/atlas").unwrap();
        let moved = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(moved.contains("`docs/atlas/quickstart.md`"));
        assert!(moved.starts_with("# House rules"));
        assert_eq!(moved.matches("<!-- ATLAS:START -->").count(), 1);
        assert_eq!(moved.matches("<!-- ATLAS:END -->").count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
