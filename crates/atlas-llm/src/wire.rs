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

/// A `tool_calls[]` entry. It is both *parsed* from the model response and
/// *echoed back* in the next request: the OpenAI schema requires the `type`
/// discriminator on the way out, and providers reject the follow-up round
/// (HTTP 400) when it is missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallPayload {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(rename = "type", default = "tool_kind")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) function: ToolCallFunction,
}

fn tool_kind() -> String {
    "function".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 工具回合的第二轮请求必须原样回传 assistant.tool_calls，且带上
    /// `"type": "function"`——缺了它严格实现（含 stepfun 兼容接口）会直接 400，
    /// 整页就退回模板。
    #[test]
    fn echoed_tool_calls_carry_the_function_type() {
        let assistant = ChatMessage {
            role: "assistant".into(),
            content: None,
            tool_calls: Some(vec![ToolCallPayload {
                id: "call_1".into(),
                kind: "function".into(),
                function: ToolCallFunction {
                    name: "read_file".into(),
                    arguments: r#"{"path":"src/lib.rs"}"#.into(),
                },
            }]),
            tool_call_id: None,
        };
        let body = serde_json::to_value(ChatMessage::tool("contents", "call_1")).unwrap();
        assert_eq!(body["role"], "tool");
        assert_eq!(body["tool_call_id"], "call_1");

        let req = ChatRequest {
            model: "m".into(),
            messages: vec![assistant],
            temperature: 0.2,
            max_tokens: 16,
            tools: vec![],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["messages"][0]["tool_calls"][0]["type"], "function");
        assert_eq!(json["messages"][0]["tool_calls"][0]["function"]["name"], "read_file");
        assert!(
            json.get("tools").is_none(),
            "an empty tool list must be omitted, not sent as []"
        );
    }

    #[test]
    fn response_tool_calls_parse_without_a_type_field() {
        let parsed: ChatResponse = serde_json::from_str(
            r#"{"choices":[{"message":{"content":null,"tool_calls":[
                 {"id":"call_1","function":{"name":"grep","arguments":"{}"}}]}}]}"#,
        )
        .unwrap();
        let calls = parsed.choices.unwrap().into_iter().next().unwrap();
        let calls = calls.message.unwrap().tool_calls.unwrap();
        assert_eq!(calls[0].function.name, "grep");
    }
}
