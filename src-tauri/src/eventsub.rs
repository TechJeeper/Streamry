//! Twitch EventSub over WebSocket (ad break start → scheduled ad end).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::auth;
use crate::engine;
use crate::AppState;

const EVENTSUB_WS: &str = "wss://eventsub.wss.twitch.tv/ws";

#[derive(Debug, Deserialize)]
struct WsEnvelope {
    metadata: WsMetadata,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WsMetadata {
    message_type: String,
    #[serde(default)]
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionPayload {
    session: SessionInfo,
}

#[derive(Debug, Deserialize)]
struct SessionInfo {
    id: String,
    #[serde(default)]
    reconnect_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdBreakEvent {
    duration_seconds: u64,
    #[serde(default)]
    broadcaster_user_name: String,
    #[serde(default)]
    broadcaster_user_login: String,
}

/// Listen for `channel.ad_break.begin` while the bot is connected.
/// Twitch has no ad-end EventSub; we schedule `ad_end` from `duration_seconds`.
pub async fn run_eventsub_loop(
    app: AppHandle,
    client_id: String,
    channel: String,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let mut url = EVENTSUB_WS.to_string();
    let ad_gen = Arc::new(AtomicU64::new(0));

    loop {
        if *stop.borrow() {
            break;
        }

        match run_session(
            &app,
            &client_id,
            &channel,
            &mut stop,
            &url,
            ad_gen.clone(),
        )
        .await
        {
            SessionOutcome::Stop => break,
            SessionOutcome::Reconnect(reconnect) => {
                url = reconnect;
            }
            SessionOutcome::Retry(err) => {
                eprintln!("EventSub: {err}");
                url = EVENTSUB_WS.to_string();
                tokio::select! {
                    _ = stop.changed() => {
                        if *stop.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
            SessionOutcome::Fatal(err) => {
                eprintln!("EventSub stopped: {err}");
                break;
            }
        }
    }

    ad_gen.fetch_add(1, Ordering::SeqCst);
}

enum SessionOutcome {
    Stop,
    Reconnect(String),
    Retry(String),
    Fatal(String),
}

async fn run_session(
    app: &AppHandle,
    client_id: &str,
    channel: &str,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    url: &str,
    ad_gen: Arc<AtomicU64>,
) -> SessionOutcome {
    let (ws, _) = match connect_async(url).await {
        Ok(v) => v,
        Err(e) => return SessionOutcome::Retry(format!("connect failed: {e}")),
    };
    let (mut write, mut read) = ws.split();

    let mut subscribed = false;

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = write.close().await;
                    return SessionOutcome::Stop;
                }
            }
            next = read.next() => {
                let msg = match next {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return SessionOutcome::Retry(format!("ws read: {e}")),
                    None => return SessionOutcome::Retry("ws closed".into()),
                };

                match msg {
                    Message::Text(text) => {
                        let envelope: WsEnvelope = match serde_json::from_str(&text) {
                            Ok(e) => e,
                            Err(e) => return SessionOutcome::Retry(format!("bad ws json: {e}")),
                        };
                        match envelope.metadata.message_type.as_str() {
                            "session_welcome" => {
                                if subscribed {
                                    // Welcome on a reconnect URL — subscriptions carry over
                                    continue;
                                }
                                let session: SessionPayload = match serde_json::from_value(envelope.payload) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        return SessionOutcome::Retry(format!("welcome payload: {e}"));
                                    }
                                };
                                match subscribe_ad_break(app, client_id, channel, &session.session.id).await {
                                    Ok(()) => subscribed = true,
                                    Err(e) => {
                                        let _ = write.close().await;
                                        return SessionOutcome::Fatal(e);
                                    }
                                }
                            }
                            "session_reconnect" => {
                                let session: SessionPayload = match serde_json::from_value(envelope.payload) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        return SessionOutcome::Retry(format!("reconnect payload: {e}"));
                                    }
                                };
                                if let Some(ru) = session.session.reconnect_url.filter(|s| !s.is_empty()) {
                                    return SessionOutcome::Reconnect(ru);
                                }
                            }
                            "notification" => {
                                if envelope.metadata.subscription_type.as_deref()
                                    == Some("channel.ad_break.begin")
                                {
                                    handle_ad_break_begin(app, &envelope.payload, ad_gen.clone());
                                }
                            }
                            "session_keepalive" | "revocation" => {}
                            _ => {}
                        }
                    }
                    Message::Ping(data) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Message::Close(_) => return SessionOutcome::Retry("ws closed by server".into()),
                    _ => {}
                }
            }
        }
    }
}

async fn subscribe_ad_break(
    app: &AppHandle,
    client_id: &str,
    channel: &str,
    session_id: &str,
) -> Result<(), String> {
    let token = auth::load_access_token(client_id).await?;
    let broadcaster_id = resolve_user_id(client_id, &token, channel).await?;

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "type": "channel.ad_break.begin",
        "version": "1",
        "condition": { "broadcaster_user_id": broadcaster_id },
        "transport": {
            "method": "websocket",
            "session_id": session_id
        }
    });

    let resp = client
        .post("https://api.twitch.tv/helix/eventsub/subscriptions")
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() || status.as_u16() == 409 {
        return Ok(());
    }

    let hint = if text.contains("authorization") || status.as_u16() == 403 {
        " Needs channel:read:ads from the broadcaster account — reconnect Twitch in Settings (streamer mode)."
    } else {
        ""
    };
    crate::activity::push(
        app,
        "automation",
        "Ad triggers",
        format!("Could not subscribe to ad breaks ({status}).{hint}"),
        "/automations",
        None,
    );
    Err(format!("subscribe ad_break failed ({status}): {text}"))
}

async fn resolve_user_id(client_id: &str, token: &str, login: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.twitch.tv/helix/users?login={}",
        urlencoding::encode(&login.to_lowercase())
    );
    let resp = client
        .get(&url)
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err("Failed to resolve channel user id.".into());
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    json.pointer("/data/0/id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Channel “{login}” not found on Twitch."))
}

fn handle_ad_break_begin(app: &AppHandle, payload: &serde_json::Value, ad_gen: Arc<AtomicU64>) {
    let event = match payload.get("event") {
        Some(e) => e,
        None => return,
    };
    let parsed: AdBreakEvent = match serde_json::from_value(event.clone()) {
        Ok(p) => p,
        Err(_) => return,
    };

    let user = if !parsed.broadcaster_user_name.is_empty() {
        parsed.broadcaster_user_name.clone()
    } else if !parsed.broadcaster_user_login.is_empty() {
        parsed.broadcaster_user_login.clone()
    } else {
        let state = app.state::<AppState>();
        let channel = state
            .runtime
            .lock()
            .channel
            .clone()
            .unwrap_or_else(|| "channel".into());
        channel
    };

    engine::fire_automations(app, "ad_start", &user);

    let duration = parsed.duration_seconds.max(1);
    let gen = ad_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let user2 = user;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration)).await;
        if ad_gen.load(Ordering::SeqCst) != gen {
            return; // a newer ad break replaced this end timer
        }
        engine::fire_automations(&app2, "ad_end", &user2);
    });
}
