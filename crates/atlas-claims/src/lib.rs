use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub statement: String,
    pub evidence: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageClaims {
    pub page: String,
    pub claims: Vec<Claim>,
    pub updated_at: Option<String>,
}

pub fn claims_dir(atlas_root: &Path) -> PathBuf {
    atlas_root.join(".claims")
}

pub fn write_page_claims(atlas_root: &Path, page: &str, claims: &[Claim]) -> Result<PathBuf> {
    let dir = claims_dir(atlas_root);
    std::fs::create_dir_all(&dir)?;
    let rel = page.replace(['/', '\\'], "__");
    let path = dir.join(format!("{rel}.json"));
    let payload = PageClaims {
        page: page.to_string(),
        claims: claims.to_vec(),
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&payload)?)?;
    Ok(path)
}

pub fn read_page_claims(atlas_root: &Path, page: &str) -> Option<PageClaims> {
    let rel = page.replace(['/', '\\'], "__");
    let path = claims_dir(atlas_root).join(format!("{rel}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Extract simple bullet claims from markdown `## Claims` section.
pub fn claims_from_markdown(page: &str, md: &str) -> Vec<Claim> {
    let mut out = Vec::new();
    let mut in_claims = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            in_claims = t.to_ascii_lowercase().contains("claim");
            continue;
        }
        if !in_claims {
            continue;
        }
        if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            if rest.len() >= 8 {
                out.push(Claim {
                    statement: rest.to_string(),
                    evidence: vec![format!("atlas://{page}")],
                    status: "active".into(),
                });
            }
        }
    }
    out
}
