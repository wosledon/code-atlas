//! 工具调用与流式写作的回归：模型把 `<tool_call><function=read_file>…` 写进正文时
//! 必须真的执行它请求的工具；边写边读时，已经写出的正文必须留下。

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

/// One streamed chunk of a fake reply: page text, or a structured tool call.
#[derive(Clone)]
enum Part {
    Text(String),
    Call { name: String, arguments: String },
}

/// A reply that is just text.
fn text(s: &str) -> Vec<Part> {
    vec![Part::Text(s.to_string())]
}

/// A reply that streams `prose` and then asks for a file — one round that writes
/// and reads at once, which is what a streaming model does.
fn prose_then_call(prose: &str, path: &str) -> Vec<Part> {
    vec![
        Part::Text(prose.to_string()),
        Part::Call {
            name: "read_file".into(),
            arguments: json!({ "path": path }).to_string(),
        },
    ]
}

/// Fake OpenAI-compatible endpoint: records every request body and answers the
/// n-th request with the n-th reply (the last one repeats).
fn fake_server(replies: Vec<Vec<Part>>, seen: Arc<Mutex<Vec<String>>>) -> String {
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

/// Same, but the reply is computed from the request body (and its index): lets a
/// test make the model react to what the client sent — a corrective instruction,
/// for example.
fn scripted_server<F>(script: F, seen: Arc<Mutex<Vec<String>>>) -> String
where
    F: Fn(&str, usize) -> Vec<Part> + Send + 'static,
{
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
            let parts = {
                let seen = seen.lock().expect("seen");
                script(&seen[index], index)
            };
            let _ = stream.write_all(&sse_response(&parts));
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

fn sse_response(parts: &[Part]) -> Vec<u8> {
    let mut body = String::new();
    for part in parts {
        let delta = match part {
            Part::Text(text) => json!({ "content": text }),
            Part::Call { name, arguments } => json!({
                "tool_calls": [{
                    "index": 0,
                    "id": "call_0",
                    "function": { "name": name, "arguments": arguments },
                }]
            }),
        };
        let payload = json!({ "choices": [{ "delta": delta }] }).to_string();
        body.push_str(&format!("data: {payload}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
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
    let llm = client(fake_server(vec![text(DIALECT), text(FINAL)], seen.clone()));
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
    let mut replies: Vec<Vec<Part>> = files.iter().map(|f| text(&dialect_for(f))).collect();
    replies.push(text(FINAL));
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

/// 只读不写：读完读取预算后不再执行，并明确告诉模型「直接写正文」——
/// 这是模型停下来的信号（只在请求里不再提供 tools，方言型模型会继续要）。
#[tokio::test]
async fn reads_without_writing_stop_at_the_read_budget() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    // 每轮换一个文件：重复调用会被去重，这里要的是「一直读新的」。
    let files = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];
    let replies: Vec<Vec<Part>> = files.iter().map(|f| text(&dialect_for(f))).collect();
    let llm = client(fake_server(replies, seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    let ran = ran.lock().expect("ran").clone();
    assert_eq!(ran.len(), 3, "读取预算 3 次，恰好用完：{ran:?}");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);

    let requests = seen.lock().expect("seen");
    // 3 次执行 + 2 次「预算已用尽」+ 2 轮无进展（注入提示词）+ 1 轮收尾。
    assert_eq!(requests.len(), 8);
    // 第 4 轮的调用被拒，拒绝文本从下一个请求起随历史上的问答一起发出。
    let refusal = &requests[4];
    assert!(
        refusal.contains("工具调用预算已用尽") && refusal.contains("直接输出页面 markdown 正文"),
        "the model must be told why the call was refused and what to do: {refusal}"
    );
    assert_eq!(
        requests[7].matches("pub fn main()").count(),
        3,
        "past the budget nothing is executed: {}",
        requests[7]
    );
}

/// 模型卡住时先自救：重复的同一轮不会立刻结束本页，而是往对话里注入纠偏提示，
/// 模型据此换做法就能把页面写完——这正是「改提示词恢复」而不是「直接截断」。
#[tokio::test]
async fn a_stuck_model_recovers_after_the_nudge() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(scripted_server(
        // 一直原地重复，直到请求里出现纠偏提示才写正文（证明是提示词救了它）。
        |request: &str, _round: usize| {
            if request.contains("系统提示") {
                vec![Part::Text("## 职责\n\n纠偏后写出的正文。\n".into())]
            } else {
                prose_then_call("## 职责\n\n正文。", "crates/x.rs")
            }
        },
        seen.clone(),
    ));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(ran.lock().expect("ran").len(), 1, "重复调用只执行一次");
    assert!(
        resp.text.contains("纠偏后写出的正文"),
        "the page must be finished after the nudge: {}",
        resp.text
    );
    let requests = seen.lock().expect("seen");
    assert!(
        requests[2].contains("系统提示"),
        "the request after the stuck round carries the corrective instruction: {}",
        requests[2]
    );
    assert_eq!(requests.len(), 3, "读一次 → 纠偏 → 写完");
}

/// 假死循环：模型反复要**同一个**文件、正文也原地重复，且对纠偏毫无反应。
/// 重复调用不执行（结果就在上文的工具消息里），两次纠偏无效后才收尾。
#[tokio::test]
async fn repeated_identical_rounds_stop_the_loop() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(
        vec![prose_then_call("## 职责\n\n正文。", "crates/x.rs")],
        seen.clone(),
    ));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(
        ran.lock().expect("ran").len(),
        1,
        "the same call runs once, not once per round"
    );
    let requests = seen.lock().expect("seen");
    assert_eq!(requests.len(), 4, "1 次执行 + 2 次纠偏 + 1 轮收尾");
    assert!(
        requests[2].contains("不会重复执行"),
        "the model is told the call is a repeat: {}",
        requests[2]
    );
    assert!(
        requests[2].contains("系统提示") && requests[3].contains("系统提示"),
        "each stuck round injects a corrective instruction"
    );
    assert!(resp.text.contains("正文。"), "{}", resp.text);
    assert_eq!(resp.text.matches("正文。").count(), 1, "{}", resp.text);
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
}

/// 一直要文件也有尽头：拒绝到上限就收尾，不会无限转下去。
#[tokio::test]
async fn endless_tool_requests_stop_at_the_cap() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(vec![text(DIALECT)], seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(ran.lock().expect("ran").len(), 1, "同一调用只执行一次");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("parameter"), "{}", resp.text);
    // 这一轮的文本是「我来确认一下」这类旁白，不是页面内容：只执行了工具、
    // 没写出页面时，返回空正文（由上层判定失败），但绝不能带上转录。
    assert!(resp.text.is_empty(), "{}", resp.text);
}

/// 边写边读：同一轮里先流出正文、再请求工具（流式模型的常态）。
/// 那段正文就是页面内容，必须留下并且只出现一次——丢掉它正是「写了又重写」的根源。
#[tokio::test]
async fn prose_streamed_before_a_tool_call_is_kept() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(
        vec![
            prose_then_call("## 职责\n\n第一段：本模块负责解析。", "crates/x.rs"),
            text(FINAL),
        ],
        seen.clone(),
    ));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(ran.lock().expect("ran").len(), 1, "the tool ran");
    assert!(resp.text.contains("第一段"), "{}", resp.text);
    assert!(resp.text.contains("定义"), "{}", resp.text);
    assert_eq!(
        resp.text.matches("第一段").count(),
        1,
        "the prose must not be duplicated by the following round: {}",
        resp.text
    );
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
}

/// 模型接着上一轮继续写（不重述）：两段都在，顺序不变。
#[tokio::test]
async fn prose_continues_across_rounds() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(
        vec![
            prose_then_call("## 职责\n\n第一段。", "crates/x.rs"),
            text("## 数据流\n\n第二段。"),
        ],
        seen.clone(),
    ));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 2, exec)
        .await
        .expect("chat_with_tools");

    assert_eq!(ran.lock().expect("ran").len(), 1, "the tool ran");
    let first = resp.text.find("第一段").expect("first paragraph");
    let second = resp.text.find("第二段").expect("second paragraph");
    assert!(first < second, "rounds keep their order: {}", resp.text);
    assert!(resp.text.contains("## 数据流"), "{}", resp.text);
}

/// 边写边读的调用**不计入读取预算**：一页写了 6 段、每段读一个文件，
/// 不能在第 3 次（`max_tool_rounds = 1` 的预算）就被掐断。
#[tokio::test]
async fn writing_rounds_do_not_charge_the_read_budget() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sections = 6;
    let mut replies: Vec<Vec<Part>> = (0..sections)
        .map(|i| {
            prose_then_call(
                &format!("## 段{i}\n\n内容 {i}。"),
                &format!("crates/f{i}.rs"),
            )
        })
        .collect();
    replies.push(text(FINAL));
    let llm = client(fake_server(replies, seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    let ran = ran.lock().expect("ran").clone();
    assert_eq!(ran.len(), sections, "每次读都该真的执行：{ran:?}");
    for i in 0..sections {
        assert!(resp.text.contains(&format!("内容 {i}。")), "{}", resp.text);
    }
    assert!(resp.text.contains("定义"), "{}", resp.text);
}

/// 一直写也一直读的模型仍然收敛：超出读取预算后继续写没问题，
/// 但每页总调用数卡在 ceiling（`max_tool_rounds × 3 × 4`）。
#[tokio::test]
async fn endless_writing_stops_at_the_call_ceiling() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    // 每轮都是新段落 + 新文件：一直「有进展」，所以只有 ceiling 能拦住它。
    let replies: Vec<Vec<Part>> = (0..20)
        .map(|i| {
            prose_then_call(
                &format!("## 段{i}\n\n内容 {i}。"),
                &format!("crates/f{i}.rs"),
            )
        })
        .collect();
    let llm = client(fake_server(replies, seen.clone()));
    let (ran, exec) = recorder();

    let resp = llm
        .chat_with_tools("system", "user", &[read_file_spec()], 1, exec)
        .await
        .expect("chat_with_tools");

    let ran = ran.lock().expect("ran").len();
    assert_eq!(ran, 12, "1 轮 × 3 次 × 上限系数 4 = 12 次调用");
    // 12 次执行 + 2 次被拒 + 1 次收尾
    assert_eq!(seen.lock().expect("seen").len(), 15);
    assert!(resp.text.contains("内容 11。"), "{}", resp.text);
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
}

/// 没有工具可给的普通问答里，转录不能被当成答案交给调用方。
#[tokio::test]
async fn plain_chat_drops_the_transcript() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = client(fake_server(vec![text(DIALECT)], seen));
    let resp = llm.chat("system", "user").await.expect("chat");
    assert!(!resp.text.contains("<tool_call"), "{}", resp.text);
    assert!(!resp.text.contains("parameter"), "{}", resp.text);
}
