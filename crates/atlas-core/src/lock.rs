use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STALE_SECS: u64 = 90;

pub struct RunLock {
    path: PathBuf,
    heartbeat: PathBuf,
    run_id: String,
}

impl RunLock {
    pub fn acquire(data_dir: &Path, run_id: &str) -> Result<Self> {
        fs::create_dir_all(data_dir)?;
        let path = data_dir.join("atlas.lock");
        let heartbeat = data_dir.join("atlas.lock.heartbeat");

        // If an existing lock is dead/stale, clear it (crash / Ctrl+C leftovers).
        if path.exists() || heartbeat.exists() {
            let holder = read_lock_holder(&path, &heartbeat);
            let stale = is_stale(&heartbeat) || holder.as_ref().map(|h| !pid_alive(h.pid)).unwrap_or(true);
            if stale {
                let _ = fs::remove_file(&path);
                let _ = fs::remove_file(&heartbeat);
            } else {
                let desc = holder
                    .map(|h| format!("pid={} run_id={}", h.pid, h.run_id))
                    .unwrap_or_else(|| "unknown".into());
                bail!(
                    "atlas lock held by another process ({desc}). \
                     If that process is gone, delete {} and retry.",
                    path.display()
                );
            }
        }

        let payload = format!(
            "{{\"pid\":{},\"run_id\":\"{}\",\"started_at\":{}}}",
            std::process::id(),
            run_id,
            now_ms()
        );
        // exclusive create
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                bail!(
                    "atlas lock already exists at {} (another run started). Delete it if stale.",
                    path.display()
                );
            }
            Err(e) => return Err(e).context("create atlas.lock"),
        };
        file.write_all(payload.as_bytes())?;
        file.flush()?;
        fs::write(&heartbeat, run_id)?;

        Ok(Self {
            path,
            heartbeat,
            run_id: run_id.to_string(),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Long runs should call this periodically so the lock is not considered stale.
    pub fn touch(&self) {
        let _ = fs::write(&self.heartbeat, self.run_id.as_bytes());
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.heartbeat);
        let _ = fs::remove_file(&self.path);
    }
}

struct LockHolder {
    pid: u32,
    run_id: String,
}

fn read_lock_holder(path: &Path, heartbeat: &Path) -> Option<LockHolder> {
    let mut pid = 0u32;
    if path.exists() {
        if let Ok(text) = fs::read_to_string(path) {
            // {"pid":123,"run_id":"..."}
            if let Some(i) = text.find("\"pid\":") {
                let rest = &text[i + 6..];
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                pid = num.parse().unwrap_or(0);
            }
        }
    }
    let run_id = fs::read_to_string(heartbeat)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if pid == 0 && run_id.is_empty() {
        return None;
    }
    Some(LockHolder { pid, run_id })
}

fn is_stale(heartbeat: &Path) -> bool {
    match fs::metadata(heartbeat).and_then(|m| m.modified()) {
        Ok(modified) => modified
            .elapsed()
            .map(|d| d > Duration::from_secs(STALE_SECS))
            .unwrap_or(true),
        Err(_) => true,
    }
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // tasklist is always available; avoid extra crates
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains(&pid.to_string())
        }
        Err(_) => true, // unknown → treat as alive (safer)
    }
}

#[cfg(not(windows))]
fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    Path::new(&format!("/proc/{pid}")).exists()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
