use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<ToolCallPayload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

impl ChatMessage {
    pub(crate) fn system(content: &str) -> Self {
        Self { role: "system".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }
    pub(crate) fn user(content: &str) -> Self {
        Self { role: "user".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }
    pub(crate) fn tool(content: &str, tool_call_id: &str) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallPayload {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) function: ToolCallFunction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ToolCallFunction {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) arguments: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolPayload<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) function: ToolFunctionPayload<'a>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolFunctionPayload<'a> {
    pub(crate) name: &'a str,
    pub(crate) description: &'a str,
    pub(crate) parameters: &'a serde_json::Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChatRequest<'a> {
    pub(crate) model: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tools: Vec<ToolPayload<'a>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatResponse {
    pub(crate) choices: Option<Vec<Choice>>,
    pub(crate) usage: Option<UsageBody>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    pub(crate) message: Option<MessageBody>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessageBody {
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) tool_calls: Option<Vec<ToolCallPayload>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageBody {
    pub(crate) prompt_tokens: Option<i64>,
    pub(crate) completion_tokens: Option<i64>,
}
