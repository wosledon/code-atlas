//! 深度重写：首稿通过结构体检后仍被判「太浅」时，让模型带着缺口清单重写全文。
//!
//! 这是**生成之后的第二次模型调用**，与首稿共用 [`super::model_call::call_model`]。
//! 重写结果只进内存，由 `pagegen` 决定是否替换已经落盘的首稿：重写期间文件保持
//! 首稿完整，这样重写失败或进程被杀都不会把已生成的内容清空。
use super::brief::depth_gaps;
use super::model_call::call_model;
use super::progress::PageBar;
use super::prompt::expand_messages;
use super::*;

/// 「扩写」：深度门发现页面太浅时，让模型带着缺口清单重写全文。
#[allow(clippy::too_many_arguments)]
async fn expand_page_with_llm(
    llm: &LlmClient,
    evidence: &str,
    page: &PlannedPage,
    cfg: &AtlasConfig,
    tools: &RepoTools,
    draft: &str,
    gaps: &[String],
) -> Result<((i64, i64, i64), String)> {
    let (system, user) = expand_messages(cfg, page, gaps, draft, evidence);
    // 只有一轮校验：草稿已带上模型读过的材料，再来一整轮工具往返等于重写一遍整页。
    call_model(llm, tools, cfg.llm.max_tool_rounds.min(1), &system, &user).await
}

/// 深度门 + 一次扩写：正文通过则原样返回，未通过则尝试重写，取缺口更少的版本。
/// 返回（最终正文, 累计用量, 是否真的重写了）。
#[allow(clippy::too_many_arguments)]
pub(super) async fn deepen_page(
    llm: &LlmClient,
    tools: &RepoTools,
    cfg: &AtlasConfig,
    page: &PlannedPage,
    evidence: &str,
    body: String,
    usage: (i64, i64, i64),
    pb: &PageBar,
    n: usize,
    total_pages: u64,
) -> (String, (i64, i64, i64), bool) {
    let gaps = depth_gaps(&body, page);
    if gaps.is_empty() {
        return (body, usage, false);
    }
    pb.note(format!(
        "扩写 {n}/{total_pages} {} · {} 项待补",
        page.rel_path,
        gaps.len()
    ));
    match atlas_llm::with_stream_sink(
        pb.clone().stream_sink(format!("扩写 {n}/{total_pages} {}", page.rel_path)),
        expand_page_with_llm(llm, evidence, page, cfg, tools, &body, &gaps),
    )
    .await
    {
        Ok((extra, revised)) => {
            let total = (usage.0 + extra.0, usage.1 + extra.1, usage.2 + extra.2);
            let after = depth_gaps(&revised, page);
            let improved = revised.trim().chars().count() >= 80
                && (after.len() < gaps.len()
                    || (after.len() == gaps.len() && revised.chars().count() > body.chars().count()));
            if improved {
                (revised, total, true)
            } else {
                (body, total, false)
            }
        }
        Err(e) => {
            tracing::warn!("[{n}/{total_pages}] {} 扩写失败: {e:#}", page.rel_path);
            (body, usage, false)
        }
    }
}
