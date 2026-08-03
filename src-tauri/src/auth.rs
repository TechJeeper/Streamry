use keyring::Entry;
use serde::Deserialize;

use crate::models::{DeviceCodeResponse, TwitchUser};

const SERVICE: &str = "Streamry";
const ACCESS: &str = "twitch_access_token";
const REFRESH: &str = "twitch_refresh_token";

#[derive(Debug, Deserialize)]
struct DeviceStart {
    device_code: String,
    expires_in: u64,
    interval: u64,
    user_code: String,
    verification_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsersResponse {
    data: Vec<HelixUser>,
}

#[derive(Debug, Deserialize)]
struct HelixUser {
    id: String,
    login: String,
    display_name: String,
}

pub async fn request_device_code(
    client_id: &str,
    scopes: &[String],
) -> Result<DeviceCodeResponse, String> {
    let client = reqwest::Client::new();
    let scope = scopes.join(" ");
    let resp = client
        .post("https://id.twitch.tv/oauth2/device")
        .form(&[("client_id", client_id), ("scopes", &scope)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request failed: {body}"));
    }
    let data: DeviceStart = resp.json().await.map_err(|e| e.to_string())?;
    Ok(DeviceCodeResponse {
        device_code: data.device_code,
        user_code: data.user_code,
        verification_uri: data.verification_uri,
        interval: data.interval,
        expires_in: data.expires_in,
    })
}

pub async fn poll_device_token(
    client_id: &str,
    device_code: &str,
    interval: u64,
) -> Result<TokenResponse, String> {
    let client = reqwest::Client::new();
    let mut wait = interval.max(1);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        let resp = client
            .post("https://id.twitch.tv/oauth2/token")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code",
                ),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        if status.is_success() {
            return serde_json::from_str(&body).map_err(|e| e.to_string());
        }
        if body.contains("authorization_pending") {
            continue;
        }
        if body.contains("slow_down") {
            wait += 5;
            continue;
        }
        return Err(format!("Authorization failed: {body}"));
    }
}

pub fn store_tokens(token: &TokenResponse) -> Result<(), String> {
    Entry::new(SERVICE, ACCESS)
        .map_err(|e| e.to_string())?
        .set_password(&token.access_token)
        .map_err(|e| e.to_string())?;
    if let Some(refresh) = &token.refresh_token {
        Entry::new(SERVICE, REFRESH)
            .map_err(|e| e.to_string())?
            .set_password(refresh)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn clear_tokens() -> Result<(), String> {
    if let Ok(e) = Entry::new(SERVICE, ACCESS) {
        let _ = e.delete_credential();
    }
    if let Ok(e) = Entry::new(SERVICE, REFRESH) {
        let _ = e.delete_credential();
    }
    Ok(())
}

pub async fn load_access_token(client_id: &str) -> Result<String, String> {
    let access = Entry::new(SERVICE, ACCESS)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|_| "Not logged in. Connect your Twitch account first.".to_string())?;

    if validate_token(&access).await {
        return Ok(access);
    }
    let refresh = Entry::new(SERVICE, REFRESH)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|_| "Session expired. Please connect Twitch again.".to_string())?;
    let token = refresh_token(client_id, &refresh).await?;
    store_tokens(&token)?;
    Ok(token.access_token)
}

async fn validate_token(token: &str) -> bool {
    let client = reqwest::Client::new();
    client
        .get("https://id.twitch.tv/oauth2/validate")
        .header("Authorization", format!("OAuth {token}"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn refresh_token(client_id: &str, refresh: &str) -> Result<TokenResponse, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err("Could not refresh Twitch session. Please reconnect.".into());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn fetch_user(client_id: &str, access: &str) -> Result<TwitchUser, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.twitch.tv/helix/users")
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {access}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err("Failed to fetch Twitch user.".into());
    }
    let users: UsersResponse = resp.json().await.map_err(|e| e.to_string())?;
    let u = users
        .data
        .into_iter()
        .next()
        .ok_or_else(|| "No Twitch user returned.".to_string())?;
    Ok(TwitchUser {
        id: u.id,
        login: u.login,
        display_name: u.display_name,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameCheckResult {
    pub ok: bool,
    pub status: String,
    pub message: String,
    pub suggested: Option<String>,
}

/// Soft availability hint via Twitch GQL (username lookup).
/// Twitch developer *app* names have no public check API — this checks whether a
/// similar login already exists as a Twitch account, which helps uniqueness.
pub async fn check_app_name_hint(name: &str) -> Result<NameCheckResult, String> {
    let trimmed = name.trim();
    if trimmed.len() < 3 || trimmed.len() > 100 {
        return Ok(NameCheckResult {
            ok: false,
            status: "invalid".into(),
            message: "Name must be between 3 and 100 characters.".into(),
            suggested: None,
        });
    }

    let login_candidate: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '-' {
                '_'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    let login_candidate = login_candidate.trim_matches('_').to_string();
    if login_candidate.len() < 4 || login_candidate.len() > 25 {
        let suggested = format!(
            "{}Bot",
            trimmed
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(12)
                .collect::<String>()
        );
        return Ok(NameCheckResult {
            ok: true,
            status: "unchecked".into(),
            message: "Twitch can’t verify this style of app name ahead of time. If Create fails, try a shorter unique name (letters/numbers) that includes your channel.".into(),
            suggested: if suggested.len() >= 4 {
                Some(suggested)
            } else {
                Some("MyChannelBot".into())
            },
        });
    }

    let client = reqwest::Client::new();
    // Twitch web Client-ID — public, used only for a username existence probe.
    let resp = client
        .post("https://gql.twitch.tv/gql")
        .header("Client-ID", "kimne78kx3ncx6brgo4mv6wki5h1ko")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": format!(
                "query {{ user(login: \"{}\") {{ id login displayName }} }}",
                login_candidate.replace('"', "")
            )
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Ok(NameCheckResult {
            ok: true,
            status: "unknown".into(),
            message: "Couldn’t reach Twitch to check. You can continue — if the name is taken when you Create, pick another.".into(),
            suggested: Some(format!("{login_candidate}_bot")),
        });
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let user = json.pointer("/data/user");
    let taken = user.map(|u| !u.is_null()).unwrap_or(false);

    if taken {
        let display = user
            .and_then(|u| u.get("displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or(&login_candidate);
        Ok(NameCheckResult {
            ok: true,
            status: "taken".into(),
            message: format!(
                "“{display}” is already a Twitch username. App names are separate, but for uniqueness try adding your channel name."
            ),
            suggested: Some(format!("{login_candidate}_bot")),
        })
    } else {
        Ok(NameCheckResult {
            ok: true,
            status: "available".into(),
            message: format!(
                "No Twitch account uses “{login_candidate}” — good unique choice for your app name."
            ),
            suggested: None,
        })
    }
}
