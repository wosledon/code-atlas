//! 对话可调用的仓库工具与一次带工具的问答。
//!
//! 分块召回按相似度取前 k 条，天然有盲区（没被切进任何 chunk 的配置键、调用点、
//! 上次运行之后新增的文件），这里的只读工具就是补这个缺口。

use atlas_core::tools::RepoTools;
use atlas_llm::ToolSpec;

/// Tools the chat may call: chunk recall answers most questions, and this pair
/// closes its blind spot — `grep` finds what the index never chunked (config
/// keys, call sites, files added since the last run) and `read_file` reads
/// around a hit. Descriptions come from [`RepoTools::specs`], so they stay
/// single-sourced with the generation tools.
pub(super) fn chat_tools() -> Vec<ToolSpec> {
    RepoTools::specs()
        .into_iter()
        .filter(|t| matches!(t.name.as_str(), "grep" | "read_file"))
        .collect()
}

/// One answered question: with the repository tools when the model supports
/// them, and a plain chat when there are none or the gateway rejects the tool
/// protocol.
pub(super) async fn answer(
    llm: &atlas_llm::LlmClient,
    tools: Option<&RepoTools>,
    specs: &[ToolSpec],
    rounds: usize,
    system: &str,
    user: &str,
) -> anyhow::Result<atlas_llm::LlmResponse> {
    if rounds > 0
        && let Some(tools) = tools
        && !specs.is_empty()
        && llm.supports_tools()
    {
        match llm
            .chat_with_tools(system, user, specs, rounds, |name, args| {
                tools.call(name, args)
            })
            .await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => tracing::warn!("对话工具回合失败，改为无工具重试：{e:#}"),
        }
    }
    llm.chat(system, user).await
}
