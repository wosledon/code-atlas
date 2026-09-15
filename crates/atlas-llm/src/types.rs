use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_temp")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_output_tokens: u32,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_temp() -> f64 {
    0.2
}

fn default_max_tokens() -> u32 {
    16384
}

fn default_timeout() -> u64 {
    180
}

fn default_retries() -> u32 {
    3
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai-compatible".into(),
            model: "qwen2.5-coder:32b".into(),
            base_url: String::new(),
            temperature: default_temp(),
            max_output_tokens: default_max_tokens(),
            timeout_secs: default_timeout(),
            retries: default_retries(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub latency_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub usage: LlmUsage,
    pub model: String,
}

/// A read-only capability the model may invoke while generating a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON schema for the arguments object.
    pub parameters: serde_json::Value,
}

/// A single `tool_calls[]` entry requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON string of the arguments object, as produced by the model.
    pub arguments: String,
}

/// One assistant turn: either final text, or a request to run tools.
#[derive(Debug, Clone)]
pub struct LlmTurn {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: LlmUsage,
    pub model: String,
}
