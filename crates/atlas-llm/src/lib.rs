//! LLM 访问层：OpenAI 兼容接口与 Anthropic Messages 接口的统一客户端。
//!
//! 模块划分：
//! - [`types`]：对外暴露的配置与结果类型（[`LlmConfig`]、[`LlmResponse`] 等）。
//! - [`wire`]：与 HTTP 接口一一对应的请求/响应 DTO。
//! - `client`：请求编排（重试、工具回合）与两种 provider 的具体传输实现。

mod client;
mod types;
mod wire;

pub use client::LlmClient;
pub use types::{LlmConfig, LlmResponse, LlmTurn, LlmUsage, ToolCall, ToolSpec};
