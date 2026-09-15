//! Progress reporting for long runs: live bars on a terminal, one plain line
//! per event when the output is redirected.
//!
//! `indicatif` can only redraw in place on a terminal — with the output
//! redirected it prints frame after frame instead, which turns a run into a
//! wall of half-drawn bars. Bars are therefore only drawn when both streams are
//! terminals (`ATLAS_PROGRESS=bars|plain` overrides the detection), and
//! anything that must not be overlapped by the next frame goes through
//! [`Progress::line`] or [`with_suspended_bars`].

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::cell::Cell;
use std::io::IsTerminal;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// The bars of the run in flight, if any: `tracing` (or anything else writing
/// to the terminal) parks them through [`with_suspended_bars`] so a log line is
/// never cut in half by the next frame. Cleared when the run closes its bars so
/// a later run (the server can pipeline several) never suspends a dead one.
static LIVE_BARS: RwLock<Option<MultiProgress>> = RwLock::new(None);

thread_local! {
    /// Suspending again while already suspended would deadlock on the bars'
    /// internal lock (a log written from inside a suspended region).
    static SUSPENDED: Cell<bool> = const { Cell::new(false) };
}

/// Run `f` with the live bars parked above the current line. A plain call when
/// no bars are drawn.
pub fn with_suspended_bars<T>(f: impl FnOnce() -> T) -> T {
    let live = LIVE_BARS.read().ok().and_then(|slot| slot.clone());
    match live {
        None => f(),
        Some(_) if SUSPENDED.with(|s| s.replace(true)) => f(),
        Some(mp) => {
            let out = mp.suspend(f);
            SUSPENDED.with(|s| s.set(false));
            out
        }
    }
}

fn set_live_bars(mp: Option<MultiProgress>) {
    if let Ok(mut slot) = LIVE_BARS.write() {
        *slot = mp;
    }
}

/// Draw bars only on a terminal: a redirected stream cannot be redrawn in
/// place, and a bar sharing a cursor with another writer garbles both.
fn draw_bars() -> bool {
    match std::env::var("ATLAS_PROGRESS").unwrap_or_default().to_ascii_lowercase().as_str() {
        "plain" | "off" | "0" => false,
        "bars" | "on" | "1" => true,
        _ => std::io::stderr().is_terminal() && std::io::stdout().is_terminal(),
    }
}

fn style(template: &'static str) -> ProgressStyle {
    ProgressStyle::with_template(template).unwrap_or_else(|_| ProgressStyle::default_spinner())
}

/// Shared handle to the two run-level bars; cloned into every page task.
#[derive(Clone)]
pub(super) struct Progress {
    inner: Arc<Inner>,
}

struct Inner {
    bars: bool,
    mp: MultiProgress,
    /// Pages that finished generating.
    generated: ProgressBar,
    /// Pages that finished their trip to `atlas/` and the index.
    landed: ProgressBar,
    total: u64,
}

/// The ticker of one page in flight.
#[derive(Clone)]
pub(super) struct PageBar {
    bars: bool,
    bar: ProgressBar,
}

impl Progress {
    pub(super) fn new(total: u64, subtitle: String) -> Self {
        let bars = draw_bars();
        let mp = MultiProgress::new();
        let (generated, landed) = if bars {
            let generated = mp.add(ProgressBar::new(total));
            generated
                .set_style(style("{spinner:.cyan} 总进度 {pos}/{len} {bar:40.cyan/blue} {msg}"));
            generated.set_message(subtitle);
            generated.enable_steady_tick(Duration::from_millis(80));
            let landed = mp.add(ProgressBar::new(total));
            landed.set_style(style("{spinner:.green} 落盘 {pos}/{len} {msg}"));
            landed.enable_steady_tick(Duration::from_millis(100));
            set_live_bars(Some(mp.clone()));
            (generated, landed)
        } else {
            (ProgressBar::hidden(), ProgressBar::hidden())
        };
        Self {
            inner: Arc::new(Inner {
                bars,
                mp,
                generated,
                landed,
                total,
            }),
        }
    }

    /// The ticker of one page; `n` is its 1-based position in the plan.
    pub(super) fn page(&self, n: usize, rel_path: &str) -> PageBar {
        let bar = if self.inner.bars {
            let bar = self.inner.mp.add(ProgressBar::new_spinner());
            bar.set_style(style("{spinner:.yellow} {msg}"));
            bar.set_message(format!("排队 {n}/{} {rel_path}", self.inner.total));
            bar.enable_steady_tick(Duration::from_millis(100));
            bar
        } else {
            ProgressBar::hidden()
        };
        PageBar {
            bars: self.inner.bars,
            bar,
        }
    }

    /// One more page finished generating.
    pub(super) fn generated(&self, finished: usize, ok: usize, failed: usize) {
        if !self.inner.bars {
            return;
        }
        let total = self.inner.total;
        self.inner.generated.set_position(finished as u64);
        self.inner
            .generated
            .set_message(format!("完成 {finished}/{total} · 成功 {ok} · 失败 {failed}"));
    }

    /// One more page finished its trip to disk (written, skipped or kept as is).
    pub(super) fn landed(&self, n: usize, rel_path: &str, detail: &str) {
        let total = self.inner.total;
        if self.inner.bars {
            self.inner.landed.inc(1);
            self.inner
                .landed
                .set_message(format!("{n}/{total} {rel_path}{detail}"));
        } else {
            println!("[atlas] 落盘 {n}/{total} {rel_path}{detail}");
        }
    }

    /// A line that must not be overlapped by the bars.
    pub(super) fn line(&self, text: &str) {
        if self.inner.bars {
            self.inner.mp.suspend(|| println!("[atlas] {text}"));
        } else {
            println!("[atlas] {text}");
        }
    }

    /// Close both run-level bars with their final numbers.
    pub(super) fn finish(&self, generated: String, landed: String) {
        if self.inner.bars {
            self.inner.generated.finish_with_message(generated);
            self.inner.landed.finish_with_message(landed);
            set_live_bars(None);
        } else {
            println!("[atlas] {generated}");
            println!("[atlas] {landed}");
        }
    }
}

impl PageBar {
    /// Transient state of the page ("撰页", "扩写", ...).
    pub(super) fn note(&self, text: String) {
        if self.bars {
            self.bar.set_message(text);
        } else {
            println!("[atlas] {text}");
        }
    }

    /// Permanent result of the page.
    pub(super) fn done(&self, text: String) {
        if self.bars {
            self.bar.finish_with_message(text);
        } else {
            println!("[atlas] {text}");
        }
    }
}
