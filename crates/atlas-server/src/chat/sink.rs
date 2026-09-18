//! 对话的回答流：把流式正文转发给浏览器，并支持撤销一个临时回合。
//!
//! 浏览器只能看见增量，无法自行判断哪一段是「临时」的，所以「哪些文本算回答」
//! 这件事必须在这里定：只有**没有工具调用**的回合才是回答。

use atlas_llm::StreamSink;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Forwards answer text to the browser, and can take back a provisional round.
///
/// A tool round streams its narration and its `<tool_call>` transcript before
/// the call runs; only a round that ends without calls is the answer. Dropping
/// it locally and re-sending the accepted text keeps half a tool transcript off
/// the reader's screen — the browser has no way to know which deltas were
/// provisional.
pub(super) struct AnswerSink {
    tx: mpsc::Sender<String>,
    state: Mutex<AnswerState>,
}

#[derive(Default)]
struct AnswerState {
    /// Accepted text — what the reader currently sees.
    text: String,
    /// Length of `text` when each open round started.
    marks: Vec<usize>,
}

impl AnswerSink {
    pub(super) fn new(tx: mpsc::Sender<String>) -> Arc<Self> {
        Arc::new(Self {
            tx,
            state: Mutex::new(AnswerState::default()),
        })
    }

    fn send(&self, frame: serde_json::Value) {
        let _ = self.tx.try_send(format!("data: {frame}\n\n"));
    }
}

impl StreamSink for AnswerSink {
    fn delta(&self, text: &str) {
        let mut state = self.state.lock().expect("answer sink");
        state.text.push_str(text);
        let _ = self.tx.try_send(format!(
            "data: {}\n\n",
            json!({"type": "delta", "text": text})
        ));
    }

    fn round_start(&self) {
        let mut state = self.state.lock().expect("answer sink");
        let len = state.text.len();
        state.marks.push(len);
    }

    fn round_end(&self, kept: bool) {
        let mut state = self.state.lock().expect("answer sink");
        let Some(mark) = state.marks.pop() else {
            return;
        };
        if kept {
            return;
        }
        // `mark` is always a char boundary: only whole deltas are pushed.
        state.text.truncate(mark);
        let rest = state.text.clone();
        self.send(json!({"type": "reset", "text": rest}));
    }
}
