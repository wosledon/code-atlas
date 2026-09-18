//! LLM 客户端：重试编排、多轮工具调用与各 provider 的传输实现。
//!
//! - [`LlmClient`]：对外入口，负责构造请求、重试与工具回合编排。
//! - `tools`：`chat_with_tools` 的多轮工具循环。
//! - `dialect`：模型把工具调用写成正文时的解析与摘除。
//! - `sse`：流式增量与 [`StreamSink`]（回合边界可回滚）。
//! - `providers`：OpenAI 兼容接口与 Anthropic Messages 接口的单轮请求。

use anyhow::{Result, anyhow};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use crate::types::{LlmConfig, LlmResponse};
use crate::wire::ChatMessage;

mod dialect;
mod providers;
mod sse;
mod tools;

pub use dialect::strip as strip_text_tool_calls;
pub use sse::{StreamSink, sink_fn, sink_pair, with_stream_sink};

pub struct LlmClient {
    cfg: LlmConfig,
    api_key: Option<String>,
    http: reqwest::Client,
    /// Prompt tokens the provider reported, and how many of them it served from
    /// its prompt cache. Both are process-local totals for this client, which is
    /// built once per run, so they answer "is the stable prefix being reused?".
    prompt_tokens: AtomicI64,
    cached_tokens: AtomicI64,
}

/// Environment variables that can carry the API key of a provider, in the order
/// they are tried. Empty for providers that never need one.
fn key_env_vars(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &["ANTHROPIC_API_KEY"],
        "host-agent" => &[],
        _ => &["OPENAI_API_KEY", "ATLAS_API_KEY"],
    }
}

impl LlmClient {
    pub fn from_env(cfg: LlmConfig) -> Result<Self> {
        // Env wins over atlas.toml so CI can inject keys without editing the repo.
        let api_key = key_env_vars(&cfg.provider)
            .iter()
            .find_map(|var| std::env::var(var).ok())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let from_cfg = cfg.api_key.trim();
                (!from_cfg.is_empty()).then(|| from_cfg.to_string())
            });
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self {
            cfg,
            api_key,
            http,
            prompt_tokens: AtomicI64::new(0),
            cached_tokens: AtomicI64::new(0),
        })
    }

    /// `(prompt tokens seen, of which served from the provider's prompt cache)`
    /// over this client's lifetime. The client is built once per run, so these
    /// are run totals; providers that do not report cache details leave the
    /// second number at zero, which is also the honest answer for them.
    pub fn prompt_cache_totals(&self) -> (i64, i64) {
        (
            self.prompt_tokens.load(Ordering::Relaxed),
            self.cached_tokens.load(Ordering::Relaxed),
        )
    }

    /// Add one turn's usage to the client totals.
    pub(super) fn account(&self, turn: &crate::types::LlmTurn) {
        self.prompt_tokens
            .fetch_add(turn.usage.prompt_tokens, Ordering::Relaxed);
        self.cached_tokens
            .fetch_add(turn.cached_tokens, Ordering::Relaxed);
    }

    pub fn model(&self) -> &str {
        &self.cfg.model
    }

    pub fn provider(&self) -> &str {
        &self.cfg.provider
    }

    pub fn is_host_agent(&self) -> bool {
        self.cfg.provider == "host-agent"
    }

    pub fn configured(&self) -> bool {
        if self.is_host_agent() || self.api_key.is_some() {
            return true;
        }
        // Keyless local runtimes (Ollama, llama.cpp, LM Studio) are usable
        // without a key; a remote base_url without a key can only 401.
        let base = self.cfg.base_url.to_lowercase();
        self.cfg.provider != "anthropic"
            && (base.is_empty()
                || base.contains("localhost")
                || base.contains("127.0.0.1")
                || base.contains("0.0.0.0"))
    }

    /// Why [`configured`](Self::configured) is false, naming the exact
    /// environment variable to set. Callers surface it so a keyless run never
    /// quietly writes structural templates over the wiki.
    pub fn unavailable_reason(&self) -> String {
        if self.is_host_agent() {
            return "provider=host-agent 由宿主 Agent 提供模型，atlas 进程无法直接调用".to_string();
        }
        let vars = key_env_vars(&self.cfg.provider).join(" 或 ");
        format!(
            "provider={} model={} base_url={}：未设置 {vars}（或 atlas.toml [llm].api_key），远程服务缺少 key 只能返回 401",
            self.cfg.provider, self.cfg.model, self.cfg.base_url
        )
    }

    /// Whether this provider supports function calling.
    pub fn supports_tools(&self) -> bool {
        !self.is_host_agent() && self.cfg.provider != "anthropic"
    }

    fn resolve_base_url(&self) -> String {
        if !self.cfg.base_url.is_empty() {
            return self.cfg.base_url.trim_end_matches('/').to_string();
        }
        match self.cfg.provider.as_str() {
            "openai" => "https://api.openai.com/v1".into(),
            _ => "http://127.0.0.1:11434/v1".into(),
        }
    }

    pub async fn chat(&self, system: &str, user: &str) -> Result<LlmResponse> {
        if self.is_host_agent() {
            return Err(anyhow!(
                "provider=host-agent expects external agent submission; direct chat unavailable"
            ));
        }
        let started = Instant::now();
        let mut resp = if self.cfg.provider == "anthropic" {
            self.chat_anthropic_with_retry(system, user, started)
                .await?
        } else {
            // `post_chat` already owns the retry budget for OpenAI-compatible
            // endpoints, so both plain chats and tool rounds retry identically.
            let messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
            let turn = self.post_chat(&messages, &[], started.elapsed()).await?;
            LlmResponse {
                text: turn.text,
                usage: turn.usage,
                model: turn.model,
            }
        };
        // This call has no tools to run, so a tool request the model wrote as
        // text is not an answer: drop it instead of handing it to the caller.
        resp.text = strip_text_tool_calls(&resp.text);
        Ok(resp)
    }

    async fn chat_anthropic_with_retry(
        &self,
        system: &str,
        user: &str,
        started: Instant,
    ) -> Result<LlmResponse> {
        let mut last_err = None;
        for attempt in 0..=self.cfg.retries {
            match self.chat_anthropic(system, user, started).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < self.cfg.retries {
                        tokio::time::sleep(Duration::from_millis(400 * (attempt as u64 + 1))).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("llm call failed")))
    }
}

pub(super) fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
