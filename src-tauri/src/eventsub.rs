//! Twitch EventSub over WebSocket (ad break start → scheduled ad end).
//! Also polls Helix Get Ad Schedule as a backup when EventSub is quiet.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::auth;
use crate::engine;
use crate::AppState;

const EVENTSUB_WS: &str = "wss://eventsub.wss.twitch.tv/ws";
/// Ignore duplicate start signals within this window (EventSub + schedule race).
const AD_DEDUP_SECS: u64 = 90;

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
    #[serde(default)]
    keepalive_timeout_seconds: Option<u64>,
}

/// Listen for `channel.ad_break.begin` while the bot is connected.
/// Twitch has no ad-end EventSub; we schedule `ad_end` from duration.
pub async fn run_eventsub_loop(
    app: AppHandle,
    client_id: String,
    channel: String,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let mut url = EVENTSUB_WS.to_string();
    let ad_gen = Arc::new(AtomicU64::new(0));
    let last_ad_fire = Arc::new(AtomicU64::new(0));
    let ads_armed = Arc::new(AtomicBool::new(false));

    // Backup: poll ad schedule only when EventSub is not armed (avoids double chat).
    {
        let app_p = app.clone();
        let client_id_p = client_id.clone();
        let channel_p = channel.clone();
        let mut stop_p = stop.clone();
        let ad_gen_p = ad_gen.clone();
        let last_ad_fire_p = last_ad_fire.clone();
        let ads_armed_p = ads_armed.clone();
        tokio::spawn(async move {
            run_ad_schedule_poller(
                app_p,
                client_id_p,
                channel_p,
                &mut stop_p,
                ad_gen_p,
                last_ad_fire_p,
                ads_armed_p,
            )
            .await;
        });
    }

    loop {
        if *stop.borrow() {
            break;
        }

        set_ads_runtime(&app, false, None);

        match run_session(
            &app,
            &client_id,
            &channel,
            &mut stop,
            &url,
            ad_gen.clone(),
            last_ad_fire.clone(),
            ads_armed.clone(),
        )
        .await
        {
            SessionOutcome::Stop => break,
            SessionOutcome::Retry(err) => {
                eprintln!("EventSub: {err}");
                ads_armed.store(false, Ordering::SeqCst);
                set_ads_runtime(&app, false, Some(err.clone()));
                url = EVENTSUB_WS.to_string();
                let wait = if err.contains("channel:read:ads")
                    || err.contains("Authorize")
                    || err.contains("streamer")
                {
                    60
                } else {
                    5
                };
                tokio::select! {
                    _ = stop.changed() => {
                        if *stop.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                }
            }
        }
    }

    ads_armed.store(false, Ordering::SeqCst);
    set_ads_runtime(&app, false, None);
    ad_gen.fetch_add(1, Ordering::SeqCst);
}

fn set_ads_runtime(app: &AppHandle, listening: bool, error: Option<String>) {
    let state = app.state::<AppState>();
    {
        let mut rt = state.runtime.lock();
        rt.ads_listening = listening;
        rt.ads_error = error;
    }
    let _ = app.emit("status-changed", state.runtime.lock().clone());
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

enum SessionOutcome {
    Stop,
    Retry(String),
}

async fn run_session(
    app: &AppHandle,
    client_id: &str,
    channel: &str,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    url: &str,
    ad_gen: Arc<AtomicU64>,
    last_ad_fire: Arc<AtomicU64>,
    ads_armed: Arc<AtomicBool>,
) -> SessionOutcome {
    // Resolve auth before connecting — Twitch only allows ~10s after welcome to subscribe.
    let ads_creds = match resolve_ads_token(client_id, channel).await {
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
            return SessionOutcome::Retry(e);
        }
    };

    let is_reconnect_url = url != EVENTSUB_WS;

    let (ws, _) = match connect_async(url).await {
        Ok(v) => v,
        Err(e) => return SessionOutcome::Retry(format!("connect failed: {e}")),
    };
    let (mut write, mut read) = ws.split();

    let mut subscribed = false;
    // Twitch default keepalive is 10s; assume dead if we go ~2× without traffic.
    let mut keepalive_secs: u64 = 10;
    let mut last_traffic = tokio::time::Instant::now();

    loop {
        let idle_limit = Duration::from_secs(keepalive_secs.saturating_mul(2).max(20));
        let idle = tokio::time::sleep_until(last_traffic + idle_limit);

        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = write.close().await;
                    return SessionOutcome::Stop;
                }
            }
            _ = idle => {
                let _ = write.close().await;
                return SessionOutcome::Retry("EventSub keepalive timeout — reconnecting".into());
            }
            next = read.next() => {
                let msg = match next {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return SessionOutcome::Retry(format!("ws read: {e}")),
                    None => return SessionOutcome::Retry("ws closed".into()),
                };
                last_traffic = tokio::time::Instant::now();

                match msg {
                    Message::Text(text) => {
                        let envelope: WsEnvelope = match serde_json::from_str(&text) {
                            Ok(e) => e,
                            Err(e) => return SessionOutcome::Retry(format!("bad ws json: {e}")),
                        };
                        match envelope.metadata.message_type.as_str() {
                            "session_welcome" => {
                                let session: SessionPayload = match serde_json::from_value(envelope.payload.clone()) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        return SessionOutcome::Retry(format!("welcome payload: {e}"));
                                    }
                                };
                                if let Some(k) = session.session.keepalive_timeout_seconds.filter(|k| *k > 0) {
                                    keepalive_secs = k;
                                }

                                // Reconnect URLs keep existing subscriptions — do not recreate.
                                if subscribed || is_reconnect_url {
                                    subscribed = true;
                                    ads_armed.store(true, Ordering::SeqCst);
                                    set_ads_runtime(app, true, None);
                                    continue;
                                }

                                match subscribe_ad_break(
                                    app,
                                    client_id,
                                    &ads_creds.0,
                                    &ads_creds.1,
                                    &session.session.id,
                                )
                                .await
                                {
                                    Ok(()) => {
                                        subscribed = true;
                                        ads_armed.store(true, Ordering::SeqCst);
                                        set_ads_runtime(app, true, None);
                                    }
                                    Err(e) => {
                                        let _ = write.close().await;
                                        return SessionOutcome::Retry(e);
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
                                    // Twitch: open the reconnect URL *before* closing this socket.
                                    match migrate_to_reconnect(
                                        &ru,
                                        &mut write,
                                        &mut read,
                                        &mut keepalive_secs,
                                        &mut last_traffic,
                                        stop,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            subscribed = true;
                                            ads_armed.store(true, Ordering::SeqCst);
                                            set_ads_runtime(app, true, None);
                                        }
                                        Err(SessionOutcome::Stop) => return SessionOutcome::Stop,
                                        Err(other) => return other,
                                    }
                                }
                            }
                            "notification" => {
                                if is_ad_break_notification(&envelope) {
                                    handle_ad_break_begin(
                                        app,
                                        &envelope.payload,
                                        ad_gen.clone(),
                                        last_ad_fire.clone(),
                                        "eventsub",
                                    );
                                }
                            }
                            "session_keepalive" => {}
                            "revocation" => {
                                ads_armed.store(false, Ordering::SeqCst);
                                set_ads_runtime(
                                    app,
                                    false,
                                    Some("Ad EventSub subscription was revoked. Reconnect the bot.".into()),
                                );
                                crate::activity::push(
                                    app,
                                    "automation",
                                    "Ad triggers",
                                    "Twitch revoked the ad subscription — reconnect the bot.",
                                    "/settings",
                                    None,
                                );
                                let _ = write.close().await;
                                return SessionOutcome::Retry("subscription revoked".into());
                            }
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

fn is_ad_break_notification(envelope: &WsEnvelope) -> bool {
    if envelope.metadata.subscription_type.as_deref() == Some("channel.ad_break.begin") {
        return true;
    }
    envelope
        .payload
        .get("subscription")
        .and_then(|s| s.get("type"))
        .and_then(|t| t.as_str())
        == Some("channel.ad_break.begin")
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;
type WsWrite = futures_util::stream::SplitSink<WsStream, Message>;
type WsRead = futures_util::stream::SplitStream<WsStream>;

/// Connect to Twitch's reconnect URL, wait for welcome, then close the old socket.
async fn migrate_to_reconnect(
    reconnect_url: &str,
    old_write: &mut WsWrite,
    old_read: &mut WsRead,
    keepalive_secs: &mut u64,
    last_traffic: &mut tokio::time::Instant,
    stop: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), SessionOutcome> {
    let (new_ws, _) = connect_async(reconnect_url)
        .await
        .map_err(|e| SessionOutcome::Retry(format!("reconnect connect failed: {e}")))?;
    let (mut new_write, mut new_read) = new_ws.split();

    // Wait for session_welcome on the new connection (subscriptions carry over).
    let welcome_deadline = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(welcome_deadline);
    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = new_write.close().await;
                    let _ = old_write.close().await;
                    return Err(SessionOutcome::Stop);
                }
            }
            _ = &mut welcome_deadline => {
                let _ = new_write.close().await;
                return Err(SessionOutcome::Retry("reconnect welcome timeout".into()));
            }
            next = new_read.next() => {
                let msg = match next {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        let _ = new_write.close().await;
                        return Err(SessionOutcome::Retry(format!("reconnect ws read: {e}")));
                    }
                    None => {
                        let _ = new_write.close().await;
                        return Err(SessionOutcome::Retry("reconnect ws closed".into()));
                    }
                };
                if let Message::Text(text) = msg {
                    let envelope: WsEnvelope = serde_json::from_str(&text).map_err(|e| {
                        SessionOutcome::Retry(format!("reconnect bad ws json: {e}"))
                    })?;
                    if envelope.metadata.message_type == "session_welcome" {
                        if let Ok(session) =
                            serde_json::from_value::<SessionPayload>(envelope.payload)
                        {
                            if let Some(k) =
                                session.session.keepalive_timeout_seconds.filter(|k| *k > 0)
                            {
                                *keepalive_secs = k;
                            }
                        }
                        break;
                    }
                } else if let Message::Ping(data) = msg {
                    let _ = new_write.send(Message::Pong(data)).await;
                }
            }
        }
    }

    // New session is live — close the old socket and swap streams in place.
    let _ = old_write.close().await;
    *old_write = new_write;
    *old_read = new_read;
    *last_traffic = tokio::time::Instant::now();
    Ok(())
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
    token: &str,
    broadcaster_id: &str,
    session_id: &str,
) -> Result<(), String> {
    match create_ad_subscription(client_id, token, broadcaster_id, session_id).await {
        Ok(()) => {
            crate::activity::push(
                app,
                "automation",
                "Ad triggers",
                "Listening for midroll ad breaks (EventSub + schedule backup).",
                "/automations",
                None,
            );
            return Ok(());
        }
        Err(CreateSubError::Conflict(existing_id)) => {
            // Stale websocket subscription for another session — delete and retry.
            let _ = delete_subscription(client_id, token, &existing_id).await;
            let _ = delete_stale_ad_subscriptions(client_id, token, broadcaster_id, session_id).await;
            match create_ad_subscription(client_id, token, broadcaster_id, session_id).await {
                Ok(()) => {
                    crate::activity::push(
                        app,
                        "automation",
                        "Ad triggers",
                        "Listening for midroll ad breaks (EventSub + schedule backup).",
                        "/automations",
                        None,
                    );
                    return Ok(());
                }
                Err(e) => {
                    let msg = format!("Could not subscribe to ad breaks after clearing conflict: {e}");
                    crate::activity::push(app, "automation", "Ad triggers", msg.clone(), "/settings", None);
                    return Err(msg);
                }
            }
        }
        Err(CreateSubError::Other(msg)) => {
            let hint = if msg.contains("authorization") || msg.contains("403") {
                " Needs channel:read:ads from the broadcaster account — reconnect Twitch (or Authorize streamer for ads) in Settings."
            } else {
                ""
            };
            let out = format!("Could not subscribe to ad breaks.{hint} ({msg})");
            crate::activity::push(app, "automation", "Ad triggers", out.clone(), "/settings", None);
            return Err(out);
        }
    }
}

enum CreateSubError {
    Conflict(String),
    Other(String),
}

impl std::fmt::Display for CreateSubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateSubError::Conflict(id) => write!(f, "conflict id={id}"),
            CreateSubError::Other(s) => write!(f, "{s}"),
        }
    }
}

async fn create_ad_subscription(
    client_id: &str,
    token: &str,
    broadcaster_id: &str,
    session_id: &str,
) -> Result<(), CreateSubError> {
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
        .map_err(|e| CreateSubError::Other(e.to_string()))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        return Ok(());
    }
    if status.as_u16() == 409 {
        // Twitch may return the existing subscription id in the body.
        let id = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("id")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        v.pointer("/data/0/id")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .unwrap_or_default();
        return Err(CreateSubError::Conflict(id));
    }
    Err(CreateSubError::Other(format!("{status}: {text}")))
}

async fn delete_subscription(client_id: &str, token: &str, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Ok(());
    }
    let client = reqwest::Client::new();
    let _ = client
        .delete(format!(
            "https://api.twitch.tv/helix/eventsub/subscriptions?id={id}"
        ))
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await;
    Ok(())
}

async fn delete_stale_ad_subscriptions(
    client_id: &str,
    token: &str,
    broadcaster_id: &str,
    current_session: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.twitch.tv/helix/eventsub/subscriptions")
        .query(&[("type", "channel.ad_break.begin")])
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Ok(());
    }
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let Some(list) = body.get("data").and_then(|d| d.as_array()) else {
        return Ok(());
    };
    for sub in list {
        let cond_id = sub
            .pointer("/condition/broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if cond_id != broadcaster_id {
            continue;
        }
        let session = sub
            .pointer("/transport/session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if session == current_session {
            continue;
        }
        if let Some(id) = sub.get("id").and_then(|v| v.as_str()) {
            let _ = delete_subscription(client_id, token, id).await;
        }
    }
    Ok(())
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

fn handle_ad_break_begin(
    app: &AppHandle,
    payload: &serde_json::Value,
    ad_gen: Arc<AtomicU64>,
    last_ad_fire: Arc<AtomicU64>,
    source: &str,
) {
    let event = match payload.get("event") {
        Some(e) => e,
        None => {
            // Some payloads are the event object itself.
            if payload.get("duration_seconds").is_some() {
                payload
            } else {
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
        }
    };

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
            let channel = state.runtime.lock().channel.clone();
            channel.unwrap_or_else(|| "channel".into())
        });

    fire_ad_start(app, &user, duration, ad_gen, last_ad_fire, source);
}

fn fire_ad_start(
    app: &AppHandle,
    user: &str,
    duration: u64,
    ad_gen: Arc<AtomicU64>,
    last_ad_fire: Arc<AtomicU64>,
    source: &str,
) {
    let now = now_unix();
    let prev = last_ad_fire.load(Ordering::SeqCst);
    if now.saturating_sub(prev) < AD_DEDUP_SECS {
        return;
    }
    last_ad_fire.store(now, Ordering::SeqCst);

    crate::activity::push(
        app,
        "automation",
        "Ad break",
        format!("Started ({duration}s via {source}) — running ad_start automations"),
        "/automations",
        None,
    );
    engine::fire_automations(app, "ad_start", user);

    let gen = ad_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let user2 = user.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration)).await;
        if ad_gen.load(Ordering::SeqCst) != gen {
            return;
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

/// Poll Helix Get Ad Schedule; fire when `last_ad_at` advances **and** EventSub is down.
async fn run_ad_schedule_poller(
    app: AppHandle,
    client_id: String,
    channel: String,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    ad_gen: Arc<AtomicU64>,
    last_ad_fire: Arc<AtomicU64>,
    ads_armed: Arc<AtomicBool>,
) {
    let mut seen_last: Option<String> = None;
    let mut primed = false;

    loop {
        if *stop.borrow() {
            break;
        }

        match resolve_ads_token(&client_id, &channel).await {
            Ok((token, broadcaster_id)) => {
                match fetch_last_ad_at(&client_id, &token, &broadcaster_id).await {
                    Ok(Some((last_at, duration))) => {
                        if !primed {
                            seen_last = Some(last_at);
                            primed = true;
                        } else if seen_last.as_ref() != Some(&last_at) {
                            seen_last = Some(last_at.clone());
                            // EventSub already covers this ad — only track, don't fire again.
                            if !ads_armed.load(Ordering::SeqCst) {
                                let user = {
                                    let state = app.state::<AppState>();
                                    let ch = state.runtime.lock().channel.clone();
                                    ch.unwrap_or_else(|| channel.clone())
                                };
                                fire_ad_start(
                                    &app,
                                    &user,
                                    duration.max(1),
                                    ad_gen.clone(),
                                    last_ad_fire.clone(),
                                    "ad schedule",
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        primed = true;
                    }
                    Err(_) => {}
                }
            }
            Err(_) => {
                // Auth errors are already reported by the EventSub loop.
            }
        }

        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
        }
    }
}

async fn fetch_last_ad_at(
    client_id: &str,
    token: &str,
    broadcaster_id: &str,
) -> Result<Option<(String, u64)>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.twitch.tv/helix/channels/ads")
        .query(&[("broadcaster_id", broadcaster_id)])
        .header("Client-Id", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("ads schedule {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let row = body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "empty ads schedule".to_string())?;

    let last = row.get("last_ad_at").ok_or_else(|| "no last_ad_at".to_string())?;
    let last_key = match last {
        serde_json::Value::String(s) if !s.is_empty() => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return Ok(None),
    };
    if last_key == "0" {
        return Ok(None);
    }

    let duration = row
        .get("duration")
        .and_then(json_u64)
        .unwrap_or(60)
        .max(1);

    Ok(Some((last_key, duration)))
}
