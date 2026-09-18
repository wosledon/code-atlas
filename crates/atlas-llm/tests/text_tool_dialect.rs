//! 文本方言工具调用的回归：模型把 `<tool_call><function=read_file>…` 写进正文时，
//! 客户端必须真的执行它请求的工具，并把转录挡在返回文本之外。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use atlas_llm::{LlmClient, LlmConfig, ToolSpec};
use serde_json::json;

/// 第一轮用 `steps` 里的文本作答，之后写正式正文。
const DIALECT: &str = "\
我来确认一下。

<tool_call>
<function=read_file>
<parameter=path>
crates/x.rs
</parameter>
<parameter=start_line>
121
</parameter>
</function>
</tool_call>
";

const FINAL: &str = "## 职责\n\n`crates/x.rs:121` 定义了入口函数，供 CLI 调用。\n";

/// Fake OpenAI-compatible endpoint: records every request body and answers the
/// first one with a text-dialect tool request, the rest with page prose.
fn fake_server(seen: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake llm");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let request = read_request(&mut stream);
            let first = {
                let mut seen = seen.lock().expect("seen");
                let first = seen.is_empty();
                seen.push(request);
                first
            };
            let reply = sse_response(if first { DIALECT } else { FINAL });
            let _ = stream.write_all(&reply);
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/v1")
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        let Some(head) = text.find("\r\n\r\n") else {
            continue;
        };
        let len = text[..head]
            .lines()
            .find_map(|l| {
                let (name, value) = l.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        if buf.len() >= head + 4 + len {
            break;
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn sse_response(text: &str) -> Vec<u8> {
    let payload = json!({ "choices": [{ "delta": { "content": text } }] }).to_string();
    let body = format!("data: {payload}\n\ndata: [DONE]\n\n");
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );
    format!("{head}{body}").into_bytes()
}

fn read_file_spec() -> ToolSpec {
    ToolSpec {
        name: "read_file".into(),
        description: "read a line window of a file".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "start_line": { "type": "integer" }
            },
            "required": ["path"]
        }),
    }
}

#[tokio::test]
async fn text_dialect_tool_request_runs_as_a_real_round() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let base_url = fake_server(seen.clone());
    let cfg = LlmConfig {
        provider: "openai-compatible".into(),
        model: "fake-model".into(),
        api_key: "test-key".into(),
        base_url,
        ..Default::default()
    };
    let llm = LlmClient::from_env(cfg).expect("client");

    let ran = Arc::new(Mutex::new(Vec::new()));
    let recorded = ran.clone();
    let resp = llm
        .chat_with_tools(
            "system",
            "user",
            &[read_file_spec()],
            2,
            move |name, args| {
                recorded.lock().expect("ran").push(format!("{name} {args}"));
                Ok("pub fn main() {}\n".into())
            },
        )
        .await
        .expect("chat_with_tools");

    assert_eq!(
        *ran.lock().expect("ran"),
        vec![r#"read_file {"path":"crates/x.rs","start_line":121}"#.to_string()],
        "the dialect call must be run, with typed arguments"
    );
    assert!(resp.text.contains("定义"), "{}", resp.text);
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("read_file"), "{}", resp.text);

    let requests = seen.lock().expect("seen");
    assert_eq!(requests.len(), 2, "one dialect round then the answer");
    let follow_up = &requests[1];
    assert!(
        follow_up.contains(r#""tool_call_id""#),
        "the tool result must answer a real call: {follow_up}"
    );
    assert!(
        follow_up.contains(r#""role":"tool""#),
        "the transcript must be continued as a tool round: {follow_up}"
    );
}

/// 没有工具可给的普通问答里，转录不能被当成答案交给调用方。
#[tokio::test]
async fn plain_chat_drops_the_transcript() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let base_url = fake_server(seen);
    let cfg = LlmConfig {
        provider: "openai-compatible".into(),
        model: "fake-model".into(),
        api_key: "test-key".into(),
        base_url,
        ..Default::default()
    };
    let llm = LlmClient::from_env(cfg).expect("client");
    let resp = llm.chat("system", "user").await.expect("chat");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("parameter"), "{}", resp.text);
}
