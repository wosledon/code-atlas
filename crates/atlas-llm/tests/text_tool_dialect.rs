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

/// 同一个协议方言，换成另一个文件——用来模拟「一轮读一个文件」。
fn dialect_for(path: &str) -> String {
    format!(
        "还要再看一个文件。\n\n<tool_call>\n<function=read_file>\n<parameter=path>\n{path}\n</parameter>\n</function>\n</tool_call>\n"
    )
}

const FINAL: &str = "## 职责\n\n`crates/x.rs:121` 定义了入口函数，供 CLI 调用。\n";

/// Fake OpenAI-compatible endpoint: records every request body and answers the
/// n-th request with the n-th reply (the last one repeats).
fn fake_server(replies: Vec<String>, seen: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake llm");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let request = read_request(&mut stream);
            let index = {
                let mut seen = seen.lock().expect("seen");
                let index = seen.len();
                seen.push(request);
                index
            };
            let reply = replies
                .get(index)
                .or_else(|| replies.last())
                .expect("at least one reply");
            let _ = stream.write_all(&sse_response(reply));
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

fn client(base_url: String) -> LlmClient {
    LlmClient::from_env(LlmConfig {
        provider: "openai-compatible".into(),
        model: "fake-model".into(),
        api_key: "test-key".into(),
        base_url,
        ..Default::default()
    })
    .expect("client")
}

type Ran = Arc<Mutex<Vec<String>>>;
type Exec = Box<dyn Fn(&str, &str) -> anyhow::Result<String>>;

/// 记录每次执行，返回一个固定的「工具结果」。
fn recorder() -> (Ran, Exec) {
    let ran: Ran = Arc::new(Mutex::new(Vec::new()));
    let recorded = ran.clone();
    let exec: Exec = Box::new(move |name: &str, args: &str| {
        recorded.lock().expect("ran").push(format!("{name} {args}"));
        Ok("pub fn main() {}\n".to_string())
    });
    (ran, exec)
}

#[tokio::test]
async fn text_dialect_tool_request_runs_as_a_real_round() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(
        vec![DIALECT.to_string(), FINAL.to_string()],
        seen.clone(),
    ));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
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

/// 一轮读一个文件的模型（线上就是它）：预算按**调用次数**算，
/// `max_tool_rounds = 2` 给出 6 次调用，足够读完一页需要的 3–6 个文件。
#[tokio::test]
async fn one_file_per_round_still_reads_a_pages_worth_of_files() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let files = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];
    let mut replies: Vec<String> = files.iter().map(|f| dialect_for(f)).collect();
    replies.push(FINAL.to_string());
    let llm = client(fake_server(replies, seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
        .await
        .expect("chat_with_tools");

    let ran = ran.lock().expect("ran").clone();
    assert_eq!(
        ran.len(),
        files.len(),
        "each requested file was read: {ran:?}"
    );
    assert!(ran[4].contains("e.rs"), "{ran:?}");
    assert!(resp.text.contains("定义"), "{}", resp.text);
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);

    // 方言生成的 id 必须跨轮唯一：重复 id 的 tool_calls 会被严格网关判 400。
    let requests = seen.lock().expect("seen");
    let last = requests.last().expect("last request");
    let mut ids: Vec<&str> = last
        .match_indices(r#""id":"text_call_"#)
        .map(|(at, _)| {
            let rest = &last[at + 6..];
            &rest[..rest.find('"').expect("closing quote")]
        })
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "call ids repeat across rounds: {ids:?}");
    assert!(total >= files.len(), "every read round is in the history");
}

/// 预算用尽后模型还在要文件：不执行，但明确告诉它「直接写正文」。
/// 这是模型停下来的信号——只在请求里不再提供 tools，方言型模型会继续要。
#[tokio::test]
async fn budget_refusal_tells_the_model_to_write() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(vec![DIALECT.to_string()], seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    let ran = ran.lock().expect("ran").clone();
    assert_eq!(ran.len(), 3, "预算 3 次，恰好用完：{ran:?}");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);

    let requests = seen.lock().expect("seen");
    assert_eq!(
        requests[3].matches("pub fn main()").count(),
        3,
        "the three budgeted calls are executed for real"
    );
    let refusal = &requests[4];
    assert_eq!(
        refusal.matches("pub fn main()").count(),
        3,
        "past the budget nothing is executed: {refusal}"
    );
    assert!(
        refusal.contains("工具调用预算已用尽") && refusal.contains("直接输出页面 markdown 正文"),
        "the model must be told why the call was refused and what to do: {refusal}"
    );
}

/// 一直要文件也有尽头：拒绝到上限就返回现有正文，不会无限转下去。
#[tokio::test]
async fn endless_tool_requests_stop_at_the_cap() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(vec![DIALECT.to_string()], seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(ran.lock().expect("ran").len(), 3, "预算内执行 3 次");
    // 3 次执行 + 1 次被告知预算已尽 + 1 次再问 + 1 次放弃
    assert_eq!(seen.lock().expect("seen").len(), 6);
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("parameter"), "{}", resp.text);
    // 走不到正文时也别把已经写出来的那段丢掉（空正文会被上层换成模板）。
    assert!(resp.text.contains("我来确认一下"), "{}", resp.text);
}

/// 没有工具可给的普通问答里，转录不能被当成答案交给调用方。
#[tokio::test]
async fn plain_chat_drops_the_transcript() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(vec![DIALECT.to_string()], seen));
    let resp = llm.chat("system", "user").await.expect("chat");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("parameter"), "{}", resp.text);
}
