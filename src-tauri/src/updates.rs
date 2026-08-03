use serde::{Deserialize, Serialize};

use crate::db;

const VERSION_URL: &str = "https://techjeeper.github.io/Streamry/version.json";
const DEFAULT_DOWNLOAD_URL: &str = "https://techjeeper.github.io/Streamry/downloads.html";
pub const DISMISSED_KEY: &str = "update_dismissed_version";

#[derive(Debug, Deserialize)]
struct RemoteVersion {
    version: String,
    #[serde(default, rename = "downloadUrl", alias = "download_url")]
    download_url: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    /// True when this latest version was dismissed for startup prompts.
    pub dismissed: bool,
    pub download_url: String,
    pub notes: Option<String>,
}

pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub async fn fetch_latest(dismissed_version: &str) -> Result<UpdateCheck, String> {
    let current = current_version();
    let url = format!(
        "{VERSION_URL}?t={}",
        chrono::Utc::now().timestamp_millis()
    );
    let remote: RemoteVersion = reqwest::Client::new()
        .get(&url)
        .header("Cache-Control", "no-cache")
        .header("Pragma", "no-cache")
        .send()
        .await
        .map_err(|e| format!("Could not check for updates: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Update check failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Invalid version file: {e}"))?;

    let latest = remote.version.trim().trim_start_matches('v').to_string();
    if latest.is_empty() {
        return Err("Update server returned an empty version.".into());
    }

    let update_available = is_newer(&latest, &current);
    let dismissed = update_available && dismissed_version.trim() == latest;

    Ok(UpdateCheck {
        current_version: current,
        latest_version: latest,
        update_available,
        dismissed,
        download_url: remote
            .download_url
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_DOWNLOAD_URL.into()),
        notes: remote.notes.filter(|n| !n.trim().is_empty()),
    })
}

pub fn dismiss(conn: &rusqlite::Connection, version: &str) -> Result<(), String> {
    let v = version.trim().trim_start_matches('v');
    if v.is_empty() {
        return Err("Missing version to dismiss.".into());
    }
    db::set_setting(conn, DISMISSED_KEY, v)
}

/// Compare dotted numeric versions (`1.2.3`). Non-numeric suffixes are ignored per segment.
fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn parse_version(s: &str) -> [u64; 3] {
    let mut out = [0u64; 3];
    for (i, part) in s.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[i] = digits.parse().unwrap_or(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0-beta", "0.0.9"));
    }
}
