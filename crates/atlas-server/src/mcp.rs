//! MCP stdio server — `atlas mcp` is the agent-facing entry.
//!
//! Newline-delimited JSON-RPC 2.0 on stdin/stdout. Logs stay on stderr.

mod tools;

use anyhow::Result;
use atlas_core::projects::ProjectRegistry;
use atlas_core::AtlasConfig;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub(crate) use tools::McpCtx;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "code-atlas";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn serve_stdio(repo_root: PathBuf, cfg: AtlasConfig) -> Result<()> {
    let atlas_root = cfg.atlas_root(&repo_root);
    let projects = ProjectRegistry::load_with_discovery(&repo_root);
    let ctx = McpCtx {
        repo_root,
        cfg,
        atlas_root,
        projects,
    };
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                write_msg(&mut stdout, &rpc_error(&Value::Null, -32700, &format!("parse error: {e}")))
                    .await?;
                continue;
            }
        };
        if let Some(resp) = handle(&ctx, &msg).await {
            write_msg(&mut stdout, &resp).await?;
        }
    }
    Ok(())
}

async fn write_msg(out: &mut (impl AsyncWriteExt + Unpin), msg: &Value) -> Result<()> {
    let mut buf = serde_json::to_vec(msg)?;
    buf.push(b'\n');
    out.write_all(&buf).await?;
    out.flush().await?;
    Ok(())
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

async fn handle(ctx: &McpCtx, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let is_notification = msg.get("id").is_none() && !method.is_empty();

    match method {
        "initialize" => Some(rpc_result(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            }),
        )),
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(rpc_result(&id, json!({}))),
        "tools/list" => Some(rpc_result(&id, json!({"tools": tools::tool_defs()}))),
        "tools/call" => {
            let name = msg.pointer("/params/name").and_then(|v| v.as_str()).unwrap_or("");
            let args = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
            Some(match tools::call_tool(ctx, name, &args).await {
                Ok(text) => rpc_result(
                    &id,
                    json!({"content": [{"type": "text", "text": text}], "isError": false}),
                ),
                Err(e) => rpc_result(
                    &id,
                    json!({"content": [{"type": "text", "text": format!("{e:#}")}], "isError": true}),
                ),
            })
        }
        "" if !is_notification && id != Value::Null => Some(rpc_error(&id, -32600, "invalid request")),
        _ if is_notification => None,
        other => Some(rpc_error(&id, -32601, &format!("method not found: {other}"))),
    }
}
