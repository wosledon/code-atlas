use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};

use crate::types::{LlmResponse, LlmTurn, LlmUsage, ToolCall, ToolSpec};
use crate::wire::{
    ChatMessage, ChatRequest, ChatResponse, MessageBody, ToolFunctionPayload, ToolPayload,
};

use super::{truncate, LlmClient};

/// Why one HTTP attempt failed, and whether it is worth trying again.
enum Attempt {
    /// Network hiccup, throttling or provider-side error: retry.
    Transient(anyhow::Error),
    /// A repeat of the same request will fail the same way (bad payload, key,
    /// model): report immediately so the page can fall back fast.
    Fatal(anyhow::Error),
}

impl Attempt {
    fn from_status(status: reqwest::StatusCode, body: &str, url: &str) -> Self {
        let code = status.as_u16();
        let err = anyhow!("llm http {status}: {} @ {url}", truncate(body, 400));
        let transient = matches!(code, 408 | 409 | 425 | 429) || code >= 500;
        if transient {
            Attempt::Transient(err)
        } else {
            Attempt::Fatal(err)
        }
    }
}

impl From<serde_json::Error> for Attempt {
    fn from(e: serde_json::Error) -> Self {
        Attempt::Fatal(anyhow!("llm response is not valid JSON: {e}"))
    }
}

impl LlmClient {
    /// One raw round-trip against an OpenAI-compatible `/chat/completions`,
    /// retrying transient failures. Tool rounds go through here as well, so a
    /// throttled follow-up call does not lose a whole page.
    pub(super) async fn post_chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        elapsed: Duration,
    ) -> Result<LlmTurn> {
        let mut last: Option<anyhow::Error> = None;
        for attempt in 0..=self.cfg.retries {
            match self.post_chat_once(messages, tools, elapsed).await {
                Ok(turn) => return Ok(turn),
                Err(Attempt::Fatal(e)) => return Err(e),
                Err(Attempt::Transient(e)) => {
                    tracing::warn!("llm 请求失败（第 {} 次）：{e:#}", attempt + 1);
                    last = Some(e);
                    if attempt < self.cfg.retries {
                        tokio::time::sleep(Duration::from_millis(
                            400 * (attempt as u64 + 1).pow(2),
                        ))
                        .await;
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("llm call failed")))
    }

    async fn post_chat_once(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        elapsed: Duration,
    ) -> std::result::Result<LlmTurn, Attempt> {
        let payloads: Vec<ToolPayload<'_>> = tools
            .iter()
            .map(|t| ToolPayload {
                kind: "function",
                function: ToolFunctionPayload {
                    name: &t.name,
                    description: &t.description,
                    parameters: &t.parameters,
                },
            })
            .collect();
        let body = ChatRequest {
            model: self.cfg.model.clone(),
            messages: messages.to_vec(),
            temperature: self.cfg.temperature,
            max_tokens: self.cfg.max_output_tokens,
            tools: payloads,
        };
        let url = format!("{}/chat/completions", self.resolve_base_url());
        let key = self
            .api_key
            .clone()
            .unwrap_or_else(|| "ollama".to_string());
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Attempt::Transient(anyhow!("llm transport error @ {url}: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| Attempt::Transient(anyhow!("llm response read error: {e}")))?;
        if !status.is_success() {
            return Err(Attempt::from_status(status, &text, &url));
        }
        let parsed: ChatResponse = serde_json::from_str(&text)?;
        let message = parsed
            .choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message)
            .unwrap_or(MessageBody {
                content: None,
                tool_calls: None,
            });
        let tool_calls = message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .filter(|(_, c)| !c.function.name.is_empty())
            .map(|(i, c)| ToolCall {
                id: if c.id.is_empty() {
                    format!("call_{i}")
                } else {
                    c.id
                },
                name: c.function.name,
                arguments: if c.function.arguments.trim().is_empty() {
                    "{}".into()
                } else {
                    c.function.arguments
                },
            })
            .collect();
        Ok(LlmTurn {
            text: message.content.unwrap_or_default(),
            tool_calls,
            usage: LlmUsage {
                prompt_tokens: parsed.usage.as_ref().and_then(|u| u.prompt_tokens).unwrap_or(0),
                completion_tokens: parsed
                    .usage
                    .as_ref()
                    .and_then(|u| u.completion_tokens)
                    .unwrap_or(0),
                latency_ms: elapsed.as_millis() as i64,
            },
            model: self.cfg.model.clone(),
        })
    }

    pub(super) async fn chat_anthropic(
        &self,
        system: &str,
        user: &str,
        started: Instant,
    ) -> Result<LlmResponse> {
        let key = self
            .api_key
            .clone()
            .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY not set"))?;
        let base = if self.cfg.base_url.is_empty() {
            "https://api.anthropic.com".to_string()
        } else {
            self.cfg.base_url.trim_end_matches('/').to_string()
        };
        let url = format!("{base}/v1/messages");
        let body = serde_json::json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_output_tokens,
            "temperature": self.cfg.temperature,
            "system": system,
            "messages": [{ "role": "user", "content": user }]
        });
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("anthropic http {status}: {}", truncate(&text, 400)));
        }
        let parsed: serde_json::Value = serde_json::from_str(&text)?;
        let content = parsed["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or_default()
            .to_string();
        Ok(LlmResponse {
            text: content,
            usage: LlmUsage {
                prompt_tokens: parsed["usage"]["input_tokens"].as_i64().unwrap_or(0),
                completion_tokens: parsed["usage"]["output_tokens"].as_i64().unwrap_or(0),
                latency_ms: started.elapsed().as_millis() as i64,
            },
            model: self.cfg.model.clone(),
        })
    }
}
