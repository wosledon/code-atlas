use std::path::Path;
use std::process::Command;

pub fn git_head(repo_root: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("--no-pager")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn git_status_short(repo_root: &Path) -> Vec<String> {
    Command::new("git")
        .args(["--no-pager", "status", "--short"])
        .current_dir(repo_root)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub fn git_log_name_status(repo_root: &Path, range: Option<&str>, max: usize) -> String {
    let mut cmd = Command::new("git");
    cmd.args(["--no-pager", "log", "--name-status", "--oneline"]);
    cmd.arg(format!("--max-count={max}"));
    if let Some(r) = range {
        cmd.arg(r);
    }
    cmd.current_dir(repo_root)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

pub fn is_git_repo(repo_root: &Path) -> bool {
    repo_root.join(".git").exists()
        || Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(repo_root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}
