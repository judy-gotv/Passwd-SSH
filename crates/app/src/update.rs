//! Auto-update check against GitHub Releases.
//!
//! Reuses the sync crate's minimal HTTP client (no extra dependencies).
//!
//! Author: Passwd Word

use opsdeck_sync::{SyncError, http_get_text};

const REPO: &str = "judy-gotv/Passwd-SSH";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Latest release info, if a newer version exists.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest: String,
    pub url: String,
    pub available: bool,
}

/// Check GitHub for a newer release.
pub async fn check_update() -> Result<UpdateInfo, SyncError> {
    let text = http_get_text(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))
    .await?;
    let v: serde_json::Value = serde_json::from_slice(text.as_bytes())
        .map_err(|e| SyncError::Malformed("release".into(), e.to_string()))?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let url = v
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or(&format!("https://github.com/{REPO}/releases"))
        .to_string();
    let available = !tag.is_empty() && version_newer(&tag, CURRENT);
    Ok(UpdateInfo {
        latest: tag,
        url,
        available,
    })
}

/// `true` when `latest` is newer than `current` (numeric dot-separated compare).
fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    let (l, c) = (parse(latest), parse(current));
    let len = l.len().max(c.len());
    for i in 0..len {
        let (a, b) = (
            l.get(i).copied().unwrap_or(0),
            c.get(i).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(version_newer("0.2.0", "0.1.0"));
        assert!(version_newer("0.1.1", "0.1.0"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(!version_newer("0.1.0", "0.2.0"));
        assert!(version_newer("1.0.0", "0.9.9"));
    }
}
