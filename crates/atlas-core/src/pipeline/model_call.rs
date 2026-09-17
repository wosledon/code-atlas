//! 生成期的一次模型调用：带只读工具回合，或工具不可用时退化为普通一问一答。
//!
//! 首稿与深度重写都经由这里，因此「工具回失败就无工具重试」的行为对两趟一致。
use super::*;

/// One model call with the repository tools attached (or a plain chat when tools
/// are disabled). Returns the token/latency usage together with the text.
pub(super) async fn call_model(
    llm: &LlmClient,
    tools: &RepoTools,
    rounds: usize,
    system: &str,
    user: &str,
) -> Result<((i64, i64, i64), String)> {
    if rounds > 0 && llm.supports_tools() {
        let specs = RepoTools::specs();
        match llm
            .chat_with_tools(system, user, &specs, rounds, |name, args| tools.call(name, args))
            .await
        {
            Ok(resp) => return Ok(usage_of(&resp)),
            // A gateway may reject the tool protocol itself (empty follow-up
            // round, schema validation, ...). Retry once tool-free: the model
            // still has the evidence bundle, and a page written from evidence
            // beats a structural template.
            Err(e) => tracing::warn!("工具回合失败，改为无工具重试：{e:#}"),
        }
    }
    Ok(usage_of(&llm.chat(system, user).await?))
}

pub(super) fn usage_of(resp: &atlas_llm::LlmResponse) -> ((i64, i64, i64), String) {
    (
        (
            resp.usage.prompt_tokens,
            resp.usage.completion_tokens,
            resp.usage.latency_ms,
        ),
        resp.text.clone(),
    )
}
