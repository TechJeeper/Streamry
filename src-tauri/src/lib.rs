use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

mod activity;
mod auth;
mod backup;
mod chat;
mod control;
mod db;
mod engine;
mod eventsub;
mod giveaway;
mod import;
mod media;
mod models;
mod overlay;
mod streamdeck;
mod tray;
mod updates;

pub use models::*;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub runtime: Mutex<RuntimeStatus>,
    pub chat_tx: Mutex<Option<mpsc::UnboundedSender<ChatOutbound>>>,
    pub bot: Mutex<BotHandle>,
    pub overlay: Arc<overlay::OverlayHub>,
    pub control: Arc<control::ControlHub>,
}

pub struct BotHandle {
    pub stop: Option<tokio::sync::watch::Sender<bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub connected: bool,
    pub connecting: bool,
    pub bot_login: Option<String>,
    pub channel: Option<String>,
    pub live: bool,
    pub last_error: Option<String>,
    pub chat_lines: u64,
    pub setup_complete: bool,
    /// True when EventSub ad-break subscription is armed.
    #[serde(default)]
    pub ads_listening: bool,
    /// Last ads/EventSub setup error (auth, subscribe, etc.).
    #[serde(default)]
    pub ads_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ChatOutbound {
    Message(String),
}

fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Streamry");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn open_db() -> Result<Connection, String> {
    let path = data_dir().join("streamry.db");
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    db::migrate(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> RuntimeStatus {
    let mut status = state.runtime.lock().clone();
    let db = state.db.lock();
    status.setup_complete = db::get_setting(&db, "setup_complete")
        .ok()
        .flatten()
        .as_deref()
        == Some("1");
    status.channel = db::get_setting(&db, "channel").ok().flatten();
    status.bot_login = db::get_setting(&db, "bot_login").ok().flatten();
    status
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.lock();
    Ok(AppSettings {
        client_id: db::get_setting(&db, "client_id")?.unwrap_or_default(),
        channel: db::get_setting(&db, "channel")?.unwrap_or_default(),
        bot_login: db::get_setting(&db, "bot_login")?.unwrap_or_default(),
        account_mode: db::get_setting(&db, "account_mode")?.unwrap_or_else(|| "streamer".into()),
        setup_complete: db::get_setting(&db, "setup_complete")?.as_deref() == Some("1"),
        confirm_giveaway_entry: db::get_setting(&db, "confirm_giveaway_entry")?.as_deref()
            != Some("0"),
        timers_live_only: db::get_setting(&db, "timers_live_only")?.as_deref() == Some("1"),
        theme: db::get_setting(&db, "theme")?
            .filter(|t| t == "light" || t == "dark")
            .unwrap_or_else(|| "dark".into()),
        stream_deck_control_enabled: db::get_setting(&db, "stream_deck_control_enabled")?
            .as_deref()
            == Some("1"),
        stream_deck_control_port: db::get_setting(&db, "stream_deck_control_port")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(control::DEFAULT_PORT),
        stream_deck_token: db::get_setting(&db, "stream_deck_token")?.unwrap_or_default(),
        streamer_ads_authorized: auth::has_ads_tokens(),
    })
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    {
        let db = state.db.lock();
        db::set_setting(&db, "client_id", &settings.client_id)?;
        db::set_setting(&db, "channel", &settings.channel.to_lowercase())?;
        db::set_setting(&db, "bot_login", &settings.bot_login.to_lowercase())?;
        db::set_setting(&db, "account_mode", &settings.account_mode)?;
        db::set_setting(
            &db,
            "setup_complete",
            if settings.setup_complete { "1" } else { "0" },
        )?;
        db::set_setting(
            &db,
            "confirm_giveaway_entry",
            if settings.confirm_giveaway_entry {
                "1"
            } else {
                "0"
            },
        )?;
        db::set_setting(
            &db,
            "timers_live_only",
            if settings.timers_live_only { "1" } else { "0" },
        )?;
        let theme = if settings.theme == "light" {
            "light"
        } else {
            "dark"
        };
        db::set_setting(&db, "theme", theme)?;
        db::set_setting(
            &db,
            "stream_deck_control_enabled",
            if settings.stream_deck_control_enabled {
                "1"
            } else {
                "0"
            },
        )?;
        let port = if settings.stream_deck_control_port == 0 {
            control::DEFAULT_PORT
        } else {
            settings.stream_deck_control_port
        };
        db::set_setting(&db, "stream_deck_control_port", &port.to_string())?;
        if !settings.stream_deck_token.is_empty() {
            db::set_setting(&db, "stream_deck_token", &settings.stream_deck_token)?;
        }
    }
    // Sync live control hub (port changes require app restart)
    state
        .control
        .set_enabled(settings.stream_deck_control_enabled);
    if !settings.stream_deck_token.is_empty() {
        state.control.set_token(settings.stream_deck_token.clone());
    }
    if settings.stream_deck_control_enabled && streamdeck::is_installed() {
        let token = state.control.token_snapshot();
        if !token.is_empty() {
            let _ = streamdeck::write_connection_file(state.control.port, &token);
        }
    }
    Ok(())
}

#[tauri::command]
async fn start_device_login(
    state: State<'_, AppState>,
    app: AppHandle,
    scopes: Vec<String>,
) -> Result<DeviceCodeResponse, String> {
    let client_id = {
        let db = state.db.lock();
        db::get_setting(&db, "client_id")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "Add your Twitch Client ID in Settings first (from dev.twitch.tv).".to_string()
            })?
    };
    let device = auth::request_device_code(&client_id, &scopes).await?;
    let user_code = device.user_code.clone();
    let device_code = device.device_code.clone();
    let interval = device.interval.max(1);
    let verification_uri = device.verification_uri.clone();

    let app2 = app.clone();
    let client_id2 = client_id.clone();
    tokio::spawn(async move {
        match auth::poll_device_token(&client_id2, &device_code, interval).await {
            Ok(token) => {
                if let Err(e) = auth::store_tokens(&token) {
                    let _ = app2.emit("auth-error", e);
                    return;
                }
                if let Ok(user) = auth::fetch_user(&client_id2, &token.access_token).await {
                    let state = app2.state::<AppState>();
                    {
                        let db = state.db.lock();
                        let _ = db::set_setting(&db, "bot_login", &user.login);
                        let mode = db::get_setting(&db, "account_mode")
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "streamer".into());
                        if mode == "streamer" {
                            let _ = db::set_setting(&db, "channel", &user.login);
                        }
                        let _ = db::set_setting(&db, "bot_user_id", &user.id);
                    }
                    let _ = app2.emit("auth-success", user);
                } else {
                    let _ = app2.emit("auth-success", serde_json::json!({ "ok": true }));
                }
            }
            Err(e) => {
                let _ = app2.emit("auth-error", e);
            }
        }
    });

    Ok(DeviceCodeResponse {
        device_code: device.device_code,
        user_code,
        verification_uri,
        interval,
        expires_in: device.expires_in,
    })
}

/// Authorize the streamer account for ad-break EventSub (used when chat runs as a separate bot).
#[tauri::command]
async fn start_streamer_ads_login(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<DeviceCodeResponse, String> {
    let (client_id, channel) = {
        let db = state.db.lock();
        (
            db::get_setting(&db, "client_id")?
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    "Add your Twitch Client ID in Settings first (from dev.twitch.tv).".to_string()
                })?,
            db::get_setting(&db, "channel")?.unwrap_or_default(),
        )
    };
    if channel.is_empty() {
        return Err("Set your channel (streamer username) first.".into());
    }

    let scopes = vec!["channel:read:ads".to_string()];
    let device = auth::request_device_code(&client_id, &scopes).await?;
    let user_code = device.user_code.clone();
    let device_code = device.device_code.clone();
    let interval = device.interval.max(1);
    let verification_uri = device.verification_uri.clone();
    let channel_expect = channel.to_lowercase();

    let app2 = app.clone();
    let client_id2 = client_id.clone();
    tokio::spawn(async move {
        match auth::poll_device_token(&client_id2, &device_code, interval).await {
            Ok(token) => match auth::fetch_user(&client_id2, &token.access_token).await {
                Ok(user) => {
                    if !user.login.eq_ignore_ascii_case(&channel_expect) {
                        let _ = app2.emit(
                            "ads-auth-error",
                            format!(
                                "Authorized as “{}”, but channel is “{channel_expect}”. Log into Twitch as the streamer and try again.",
                                user.login
                            ),
                        );
                        return;
                    }
                    if let Err(e) = auth::store_ads_tokens(&token) {
                        let _ = app2.emit("ads-auth-error", e);
                        return;
                    }
                    let _ = app2.emit("ads-auth-success", user);
                }
                Err(e) => {
                    let _ = app2.emit("ads-auth-error", e);
                }
            },
            Err(e) => {
                let _ = app2.emit("ads-auth-error", e);
            }
        }
    });

    Ok(DeviceCodeResponse {
        device_code: device.device_code,
        user_code,
        verification_uri,
        interval,
        expires_in: device.expires_in,
    })
}

#[tauri::command]
fn logout(state: State<'_, AppState>) -> Result<(), String> {
    auth::clear_tokens()?;
    {
        let mut rt = state.runtime.lock();
        rt.connected = false;
        rt.connecting = false;
    }
    if let Some(tx) = state.bot.lock().stop.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
async fn connect_bot(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    start_bot_connection(&state, app).await
}

async fn start_bot_connection(state: &AppState, app: AppHandle) -> Result<(), String> {
    {
        let mut rt = state.runtime.lock();
        if rt.connected || rt.connecting {
            return Ok(());
        }
        rt.connecting = true;
        rt.last_error = None;
    }

    let (client_id, channel, bot_login) = {
        let db = state.db.lock();
        (
            db::get_setting(&db, "client_id")?.unwrap_or_default(),
            db::get_setting(&db, "channel")?.unwrap_or_default(),
            db::get_setting(&db, "bot_login")?.unwrap_or_default(),
        )
    };

    if client_id.is_empty() || channel.is_empty() {
        state.runtime.lock().connecting = false;
        return Err("Connect Twitch and set a channel first.".into());
    }

    let token = match auth::load_access_token(&client_id).await {
        Ok(t) => t,
        Err(e) => {
            state.runtime.lock().connecting = false;
            return Err(e);
        }
    };
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let (chat_tx, chat_rx) = mpsc::unbounded_channel();
    *state.chat_tx.lock() = Some(chat_tx);
    state.bot.lock().stop = Some(stop_tx);

    let app2 = app.clone();
    let channel2 = channel.clone();
    let bot_login2 = if bot_login.is_empty() {
        channel.clone()
    } else {
        bot_login
    };
    let bot_login_for_status = bot_login2.clone();
    let client_id_es = client_id.clone();
    let channel_es = channel.clone();
    let stop_rx_es = stop_rx.clone();

    tokio::spawn(async move {
        eventsub::run_eventsub_loop(app2, client_id_es, channel_es, stop_rx_es).await;
    });

    let app2 = app.clone();
    tokio::spawn(async move {
        let result = chat::run_chat_loop(
            app2.clone(),
            client_id,
            token,
            bot_login2,
            channel2,
            chat_rx,
            stop_rx,
        )
        .await;
        let state = app2.state::<AppState>();
        let mut rt = state.runtime.lock();
        rt.connected = false;
        rt.connecting = false;
        if let Err(e) = result {
            rt.last_error = Some(e.clone());
            let _ = app2.emit("bot-error", e);
        }
        let _ = app2.emit("status-changed", rt.clone());
    });

    {
        let mut rt = state.runtime.lock();
        rt.connected = true;
        rt.connecting = false;
        rt.channel = Some(channel);
        rt.bot_login = Some(bot_login_for_status);
    }
    let _ = app.emit("status-changed", state.runtime.lock().clone());
    Ok(())
}

#[tauri::command]
fn disconnect_bot(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let _ = state;
    control_disconnect_bot(app)
}

pub async fn control_connect_bot(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    start_bot_connection(state.inner(), app.clone()).await
}

pub fn control_disconnect_bot(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(tx) = state.bot.lock().stop.take() {
        let _ = tx.send(true);
    }
    *state.chat_tx.lock() = None;
    {
        let mut rt = state.runtime.lock();
        rt.connected = false;
        rt.connecting = false;
    }
    let _ = app.emit("status-changed", state.runtime.lock().clone());
    Ok(())
}

#[tauri::command]
fn send_chat(state: State<'_, AppState>, app: AppHandle, message: String) -> Result<(), String> {
    let msg = message.trim().to_string();
    if msg.is_empty() {
        return Err("Message is empty".into());
    }
    let tx = state.chat_tx.lock();
    tx.as_ref()
        .ok_or_else(|| "Bot is not connected.".to_string())?
        .send(ChatOutbound::Message(msg.clone()))
        .map_err(|e| e.to_string())?;
    activity::push(
        &app,
        "chat",
        "Bot message",
        msg,
        "/",
        None,
    );
    Ok(())
}

// ---- Commands CRUD ----
#[tauri::command]
fn list_commands(state: State<'_, AppState>) -> Result<Vec<ChatCommand>, String> {
    db::list_commands(&state.db.lock())
}

#[tauri::command]
fn upsert_command(state: State<'_, AppState>, cmd: ChatCommand) -> Result<ChatCommand, String> {
    db::upsert_command(&state.db.lock(), cmd)
}

#[tauri::command]
fn delete_command(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_command(&state.db.lock(), &id)
}

// ---- Timers CRUD ----
#[tauri::command]
fn list_timers(state: State<'_, AppState>) -> Result<Vec<ChatTimer>, String> {
    db::list_timers(&state.db.lock())
}

#[tauri::command]
fn upsert_timer(state: State<'_, AppState>, timer: ChatTimer) -> Result<ChatTimer, String> {
    db::upsert_timer(&state.db.lock(), timer)
}

#[tauri::command]
fn delete_timer(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_timer(&state.db.lock(), &id)
}

// ---- Giveaways ----
#[tauri::command]
fn list_giveaways(state: State<'_, AppState>) -> Result<Vec<Giveaway>, String> {
    db::list_giveaways(&state.db.lock())
}

#[tauri::command]
fn upsert_giveaway(state: State<'_, AppState>, gw: Giveaway) -> Result<Giveaway, String> {
    db::upsert_giveaway(&state.db.lock(), gw)
}

#[tauri::command]
fn delete_giveaway(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_giveaway(&state.db.lock(), &id)
}

#[tauri::command]
fn get_active_giveaway(state: State<'_, AppState>) -> Result<Option<ActiveGiveawayView>, String> {
    giveaway::get_active_view(&state.db.lock())
}

#[tauri::command]
fn list_giveaway_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<GiveawayRunHistory>, String> {
    giveaway::list_winner_history(&state.db.lock(), limit.unwrap_or(50))
}

#[tauri::command]
fn start_giveaway(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    giveaway::start(&state.db.lock(), &id)?;
    let title = db::list_giveaways(&state.db.lock())
        .ok()
        .and_then(|list| list.into_iter().find(|g| g.id == id).map(|g| g.title))
        .unwrap_or_else(|| "Giveaway".into());
    activity::push(
        &app,
        "giveaway",
        title,
        "Started",
        "/giveaways",
        Some(id),
    );
    let _ = app.emit("giveaway-updated", ());
    Ok(())
}

#[tauri::command]
fn stop_giveaway(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let active = giveaway::get_active_view(&state.db.lock())?;
    giveaway::stop_active(&state.db.lock())?;
    if let Some(a) = active {
        activity::push(
            &app,
            "giveaway",
            a.giveaway.title,
            "Stopped",
            "/giveaways",
            Some(a.giveaway.id),
        );
    }
    let _ = app.emit("giveaway-updated", ());
    Ok(())
}

#[tauri::command]
fn draw_giveaway(state: State<'_, AppState>, app: AppHandle) -> Result<Vec<GiveawayWinner>, String> {
    let active = giveaway::get_active_view(&state.db.lock())?;
    let winners = giveaway::draw_winners(&state.db.lock())?;
    if !winners.is_empty() {
        let announce = {
            let db = state.db.lock();
            giveaway::format_announce(&db, &winners)?
        };
        if let Some(tx) = state.chat_tx.lock().clone() {
            let _ = tx.send(ChatOutbound::Message(announce));
        }
        if let Some(a) = &active {
            let names = winners
                .iter()
                .map(|w| format!("@{}", w.login))
                .collect::<Vec<_>>()
                .join(", ");
            activity::push(
                &app,
                "giveaway",
                a.giveaway.title.clone(),
                format!("Winner(s): {names}"),
                "/giveaways",
                Some(a.giveaway.id.clone()),
            );
        }
    }
    let _ = app.emit("giveaway-updated", ());
    Ok(winners)
}

// ---- Automations ----
#[tauri::command]
fn list_automations(state: State<'_, AppState>) -> Result<Vec<Automation>, String> {
    db::list_automations(&state.db.lock())
}

#[tauri::command]
fn upsert_automation(
    state: State<'_, AppState>,
    auto: Automation,
) -> Result<Automation, String> {
    db::upsert_automation(&state.db.lock(), auto)
}

#[tauri::command]
fn delete_automation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_automation(&state.db.lock(), &id)
}

#[tauri::command]
fn test_automation(app: AppHandle, id: String) -> Result<(), String> {
    engine::run_automation_by_id(&app, &id)
}

// ---- Media / OBS overlay ----
#[tauri::command]
fn get_overlay_info(state: State<'_, AppState>) -> OverlayInfo {
    OverlayInfo {
        url: state.overlay.browser_url(),
        port: state.overlay.port(),
        running: true,
    }
}

#[tauri::command]
fn list_media(state: State<'_, AppState>) -> Result<Vec<MediaClip>, String> {
    db::list_media(&state.db.lock())
}

#[tauri::command]
fn import_media(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<MediaClip, String> {
    let (clip, _) = media::import_file(std::path::Path::new(&path), name)?;
    db::upsert_media(&state.db.lock(), clip)
}

#[tauri::command]
fn upsert_media(state: State<'_, AppState>, clip: MediaClip) -> Result<MediaClip, String> {
    db::upsert_media(&state.db.lock(), clip)
}

#[tauri::command]
fn delete_media(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Some(clip) = db::delete_media(&state.db.lock(), &id)? {
        media::delete_file(&clip.file_name);
    }
    Ok(())
}

#[tauri::command]
fn test_media(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let clip = db::get_media(&state.db.lock(), &id)?
        .ok_or_else(|| "Media clip not found".to_string())?;
    media::play_clip(&state.overlay, &clip);
    activity::push(
        &app,
        "media",
        clip.name.clone(),
        format!("Test played {} on overlay", clip.media_type),
        "/media",
        Some(clip.id),
    );
    Ok(())
}

// ---- Variables ----
#[tauri::command]
fn list_variables(state: State<'_, AppState>) -> Result<Vec<CustomVariable>, String> {
    db::list_variables(&state.db.lock())
}

#[tauri::command]
fn upsert_variable(
    state: State<'_, AppState>,
    var: CustomVariable,
) -> Result<CustomVariable, String> {
    db::upsert_variable(&state.db.lock(), var)
}

#[tauri::command]
fn delete_variable(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_variable(&state.db.lock(), &id)
}

// ---- Import / Backup ----
#[tauri::command]
fn parse_streamelements_zip(path: String) -> Result<SeImportPreview, String> {
    import::parse_se_zip(&path)
}

#[tauri::command]
fn import_streamelements(
    state: State<'_, AppState>,
    path: String,
    command_ids: Vec<String>,
    timer_ids: Vec<String>,
    variable_ids: Vec<String>,
    automation_ids: Vec<String>,
    on_collision: String,
) -> Result<ImportResult, String> {
    import::import_selected(
        &state.db.lock(),
        &path,
        &command_ids,
        &timer_ids,
        &variable_ids,
        &automation_ids,
        &on_collision,
    )
}

#[tauri::command]
fn export_backup(state: State<'_, AppState>, path: String) -> Result<(), String> {
    backup::export_backup(&state.db.lock(), &path)
}

#[tauri::command]
fn preview_backup(path: String) -> Result<BackupPreview, String> {
    backup::preview_backup(&path)
}

#[tauri::command]
fn restore_backup(
    state: State<'_, AppState>,
    path: String,
    include_commands: bool,
    include_timers: bool,
    include_giveaways: bool,
    include_automations: bool,
    include_variables: bool,
    replace: bool,
) -> Result<ImportResult, String> {
    backup::restore_backup(
        &state.db.lock(),
        &path,
        include_commands,
        include_timers,
        include_giveaways,
        include_automations,
        include_variables,
        replace,
    )
}

#[tauri::command]
fn complete_setup(state: State<'_, AppState>) -> Result<(), String> {
    db::set_setting(&state.db.lock(), "setup_complete", "1")
}

#[tauri::command]
fn get_app_version() -> String {
    updates::current_version()
}

#[tauri::command]
async fn check_for_update(state: State<'_, AppState>) -> Result<updates::UpdateCheck, String> {
    let dismissed = db::get_setting(&state.db.lock(), updates::DISMISSED_KEY)?.unwrap_or_default();
    updates::fetch_latest(&dismissed).await
}

#[tauri::command]
fn dismiss_update(state: State<'_, AppState>, version: String) -> Result<(), String> {
    updates::dismiss(&state.db.lock(), &version)
}

#[tauri::command]
fn reset_app(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    // Stop live bot connection
    {
        let mut rt = state.runtime.lock();
        rt.connected = false;
        rt.connecting = false;
        rt.bot_login = None;
        rt.channel = None;
        rt.last_error = None;
        rt.chat_lines = 0;
        rt.live = false;
    }
    if let Some(tx) = state.bot.lock().stop.take() {
        let _ = tx.send(true);
    }

    // Close any active giveaway run
    let _ = giveaway::stop_active(&state.db.lock());

    // Remove imported media files from disk
    {
        let clips = db::list_media(&state.db.lock()).unwrap_or_default();
        for clip in clips {
            media::delete_file(&clip.file_name);
        }
        if let Ok(entries) = std::fs::read_dir(media::media_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    auth::clear_tokens()?;
    db::factory_reset(&state.db.lock())?;

    let _ = app.emit("status-changed", state.runtime.lock().clone());
    let _ = app.emit("app-reset", ());
    Ok(())
}

#[tauri::command]
async fn check_app_name(name: String) -> Result<auth::NameCheckResult, String> {
    auth::check_app_name_hint(&name).await
}

#[tauri::command]
fn get_streamdeck_status(state: State<'_, AppState>) -> StreamDeckStatus {
    streamdeck::status(&state)
}

#[tauri::command]
fn install_streamdeck_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StreamDeckStatus, String> {
    streamdeck::install_plugin(&app, &state)
}

#[tauri::command]
fn set_streamdeck_control(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<StreamDeckStatus, String> {
    streamdeck::set_control_enabled(&state, enabled)
}

fn startup_log(msg: &str) {
    let path = data_dir().join("startup.log");
    let line = format!("{} {}\n", chrono::Local::now().to_rfc3339(), msg);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    startup_log("run() starting");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            startup_log("setup begin");
            let conn = match open_db() {
                Ok(c) => c,
                Err(e) => {
                    startup_log(&format!("db open failed: {e}"));
                    return Err(e.into());
                }
            };

            let overlay_port = {
                let port = db::get_setting(&conn, "overlay_port")
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(overlay::DEFAULT_PORT);
                port
            };
            let hub = Arc::new(overlay::OverlayHub::new(media::media_dir(), overlay_port));
            {
                let hub_clone = hub.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = overlay::start_server(hub_clone).await {
                        eprintln!("overlay server: {e}");
                    }
                });
            }

            let control_port = db::get_setting(&conn, "stream_deck_control_port")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(control::DEFAULT_PORT);
            let control_enabled = db::get_setting(&conn, "stream_deck_control_enabled")
                .ok()
                .flatten()
                .as_deref()
                == Some("1");
            let control_token = db::get_setting(&conn, "stream_deck_token")
                .ok()
                .flatten()
                .unwrap_or_default();
            let control_hub = Arc::new(control::ControlHub::new(
                control_port,
                control_enabled,
                control_token,
            ));
            {
                let control_clone = control_hub.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = control::start_server(handle, control_clone).await {
                        eprintln!("control API: {e}");
                    }
                });
            }

            app.manage(AppState {
                db: Mutex::new(conn),
                runtime: Mutex::new(RuntimeStatus::default()),
                chat_tx: Mutex::new(None),
                bot: Mutex::new(BotHandle { stop: None }),
                overlay: hub,
                control: control_hub,
            });
            if let Err(e) = tray::setup_tray(app.handle()) {
                startup_log(&format!("tray setup failed (continuing): {e}"));
            } else {
                startup_log("tray ok");
            }

            // Close to tray
            let handle = app.handle().clone();
            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                });
                startup_log("window hooks ok");
            } else {
                startup_log("main window missing");
            }

            // Background scheduler for timers + giveaway countdown
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                engine::run_scheduler(handle).await;
            });

            // Auto-connect once setup + Twitch credentials are present
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                let state = handle.state::<AppState>();
                let should = {
                    let db = state.db.lock();
                    let setup = db::get_setting(&db, "setup_complete")
                        .ok()
                        .flatten()
                        .as_deref()
                        == Some("1");
                    let client = db::get_setting(&db, "client_id")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let channel = db::get_setting(&db, "channel")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    setup && !client.is_empty() && !channel.is_empty()
                };
                if should {
                    if let Err(e) = start_bot_connection(state.inner(), handle.clone()).await {
                        startup_log(&format!("auto-connect skipped: {e}"));
                    } else {
                        startup_log("auto-connect ok");
                    }
                }
            });
            startup_log("setup complete");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_settings,
            save_settings,
            start_device_login,
            start_streamer_ads_login,
            logout,
            connect_bot,
            disconnect_bot,
            send_chat,
            list_commands,
            upsert_command,
            delete_command,
            list_timers,
            upsert_timer,
            delete_timer,
            list_giveaways,
            upsert_giveaway,
            delete_giveaway,
            get_active_giveaway,
            list_giveaway_history,
            start_giveaway,
            stop_giveaway,
            draw_giveaway,
            list_automations,
            upsert_automation,
            delete_automation,
            test_automation,
            get_overlay_info,
            list_media,
            import_media,
            upsert_media,
            delete_media,
            test_media,
            list_variables,
            upsert_variable,
            delete_variable,
            parse_streamelements_zip,
            import_streamelements,
            export_backup,
            preview_backup,
            restore_backup,
            complete_setup,
            get_app_version,
            check_for_update,
            dismiss_update,
            reset_app,
            check_app_name,
            get_streamdeck_status,
            install_streamdeck_plugin,
            set_streamdeck_control,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Streamry");
}
