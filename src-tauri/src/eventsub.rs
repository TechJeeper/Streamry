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

/// Prefer a dedicated streamer ads token (bot mode); else the main token if it is the broadcaster.
async fn resolve_ads_token(client_id: &str, channel: &str) -> Result<(String, String), String> {
    let channel_lc = channel.to_lowercase();

    if let Ok(token) = auth::load_ads_access_token(client_id).await {
        match auth::token_info(&token).await {
            Ok(info) if info.login.eq_ignore_ascii_case(&channel_lc) => {
                if !info.scopes.iter().any(|s| s == "channel:read:ads") {
                    return Err(
                        "Streamer ads login is missing channel:read:ads. Authorize the streamer again in Settings."
                            .into(),
                    );
                }
                return Ok((token, info.user_id));
            }
            Ok(info) => {
                return Err(format!(
                    "Ads authorization is for “{}”, but the channel is “{channel}”. Authorize while logged in as the streamer.",
                    info.login
                ));
            }
            Err(_) => {}
        }
    }

    let token = auth::load_access_token(client_id).await?;
    let info = auth::token_info(&token).await?;
    if !info.login.eq_ignore_ascii_case(&channel_lc) {
        return Err(format!(
            "Ad triggers need the streamer’s Twitch login. Chat is connected as “{}”. In Settings, use “Authorize streamer for ads” while logged into Twitch as {channel}.",
            info.login
        ));
    }
    if !info.scopes.iter().any(|s| s == "channel:read:ads") {
        return Err(
            "Missing channel:read:ads. Reconnect Twitch in Settings so the ads permission is granted."
                .into(),
        );
    }
    Ok((token, info.user_id))
}

async fn subscribe_ad_break(
    app: &AppHandle,
    client_id: &str,
    channel: &str,
    session_id: &str,
) -> Result<(), String> {
    let (token, broadcaster_id) = match resolve_ads_token(client_id, channel).await {
        Ok(v) => v,
        Err(e) => {
            crate::activity::push(
                app,
                "automation",
                "Ad triggers",
                e.clone(),
                "/settings",
                None,
            );
            return Err(e);
        }
    };

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
        crate::activity::push(
            app,
            "automation",
            "Ad triggers",
            "Listening for midroll ad breaks (start + end).",
            "/automations",
            None,
        );
        return Ok(());
    }

    let hint = if text.contains("authorization") || status.as_u16() == 403 {
        " Needs channel:read:ads from the broadcaster account — reconnect Twitch (or Authorize streamer for ads) in Settings."
    } else {
        ""
    };
    let msg = format!("Could not subscribe to ad breaks ({status}).{hint}");
    crate::activity::push(app, "automation", "Ad triggers", msg.clone(), "/settings", None);
    Err(format!("subscribe ad_break failed ({status}): {text}"))
}

fn json_u64(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().and_then(|i| u64::try_from(i).ok()))
            .or_else(|| n.as_f64().map(|f| f as u64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn json_str(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(|s| s.to_string())
}

fn handle_ad_break_begin(app: &AppHandle, payload: &serde_json::Value, ad_gen: Arc<AtomicU64>) {
    let event = match payload.get("event") {
        Some(e) => e,
        None => {
            eprintln!("EventSub ad_break: notification missing event field: {payload}");
            crate::activity::push(
                app,
                "automation",
                "Ad triggers",
                "Received an ad event with no payload — ignored.",
                "/automations",
                None,
            );
            return;
        }
    };

    // Twitch docs show duration_seconds as a string; live payloads may be numbers.
    let duration = event
        .get("duration_seconds")
        .and_then(json_u64)
        .unwrap_or(60)
        .max(1);

    let user = event
        .get("broadcaster_user_name")
        .and_then(json_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            event
                .get("broadcaster_user_login")
                .and_then(json_str)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| {
            let state = app.state::<AppState>();
            let channel = state
                .runtime
                .lock()
                .channel
                .clone()
                .unwrap_or_else(|| "channel".into());
            channel
        });

    crate::activity::push(
        app,
        "automation",
        "Ad break",
        format!("Started ({duration}s) — running ad_start automations"),
        "/automations",
        None,
    );
    engine::fire_automations(app, "ad_start", &user);

    let gen = ad_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let user2 = user;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration)).await;
        if ad_gen.load(Ordering::SeqCst) != gen {
            return; // a newer ad break replaced this end timer
        }
        crate::activity::push(
            &app2,
            "automation",
            "Ad break",
            "Ended — running ad_end automations",
            "/automations",
            None,
        );
        engine::fire_automations(&app2, "ad_end", &user2);
    });
}
