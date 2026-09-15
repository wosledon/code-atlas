//! LLM 客户端：重试编排、多轮工具调用与各 provider 的传输实现。
//!
//! - [`LlmClient`]：对外入口，负责构造请求、重试与工具回合编排。
//! - `tools`：`chat_with_tools` 的多轮工具循环。
//! - `providers`：OpenAI 兼容接口与 Anthropic Messages 接口的单轮请求。

use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};

use crate::types::{LlmConfig, LlmResponse};
use crate::wire::ChatMessage;

mod providers;
mod tools;

pub struct LlmClient {
    cfg: LlmConfig,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn from_env(cfg: LlmConfig) -> Result<Self> {
        let api_key = match cfg.provider.as_str() {
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
            "host-agent" => None,
            _ => std::env::var("OPENAI_API_KEY")
                .ok()
                .or_else(|| std::env::var("ATLAS_API_KEY").ok()),
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self {
            cfg,
            api_key,
            http,
        })
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
        let mut last_err = None;
        for attempt in 0..=self.cfg.retries {
            match self.chat_once(system, user).await {
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

    async fn chat_once(&self, system: &str, user: &str) -> Result<LlmResponse> {
        let started = Instant::now();
        if self.cfg.provider == "anthropic" {
            return self.chat_anthropic(system, user, started).await;
        }
        let messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
        let turn = self
            .post_chat(&messages, &[], started.elapsed())
            .await?;
        Ok(LlmResponse {
            text: turn.text,
            usage: turn.usage,
            model: turn.model,
        })
    }

}

pub(super) fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
