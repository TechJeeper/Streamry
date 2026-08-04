use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tower_http::cors::{Any, CorsLayer};

use crate::db;
use crate::engine;
use crate::giveaway;
use crate::media;
use crate::{AppState, ChatOutbound};

pub const DEFAULT_PORT: u16 = 1920;
pub const TOKEN_HEADER: &str = "x-streamry-token";

#[derive(Clone)]
pub struct ControlHub {
    pub enabled: Arc<AtomicBool>,
    pub token: Arc<Mutex<String>>,
    pub port: u16,
}

impl ControlHub {
    pub fn new(port: u16, enabled: bool, token: String) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            token: Arc::new(Mutex::new(token)),
            port,
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_token(&self, token: String) {
        *self.token.lock() = token;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn token_snapshot(&self) -> String {
        self.token.lock().clone()
    }
}

#[derive(Clone)]
struct ApiState {
    app: AppHandle,
    hub: Arc<ControlHub>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponse {
    ok: bool,
    connected: bool,
    connecting: bool,
    channel: Option<String>,
    version: String,
    control_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdNameEnabled {
    id: String,
    name: String,
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomationItem {
    id: String,
    name: String,
    enabled: bool,
    trigger_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GiveawayItem {
    id: String,
    title: String,
    enabled: bool,
    active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaItem {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Deserialize)]
struct ChatBody {
    message: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorBody>) {
    (status, Json(ErrorBody { error: msg.into() }))
}

fn require_auth(headers: &HeaderMap, hub: &ControlHub) -> Result<(), (StatusCode, Json<ErrorBody>)> {
    if !hub.is_enabled() {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Stream Deck control API is disabled",
        ));
    }
    let expected = hub.token_snapshot();
    if expected.is_empty() {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Stream Deck control token not configured",
        ));
    }
    let provided = headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(err(StatusCode::UNAUTHORIZED, "Invalid or missing token"));
    }
    Ok(())
}

async fn get_status(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<StatusResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let rt = app_state.runtime.lock().clone();
    Ok(Json(StatusResponse {
        ok: true,
        connected: rt.connected,
        connecting: rt.connecting,
        channel: rt.channel,
        version: env!("CARGO_PKG_VERSION").into(),
        control_enabled: state.hub.is_enabled(),
    }))
}

async fn list_commands(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<IdNameEnabled>>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let cmds = db::list_commands(&app_state.db.lock()).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(
        cmds.into_iter()
            .map(|c| IdNameEnabled {
                id: c.id,
                name: c.name,
                enabled: c.enabled,
            })
            .collect(),
    ))
}

async fn list_automations(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AutomationItem>>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let autos =
        db::list_automations(&app_state.db.lock()).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(
        autos
            .into_iter()
            .map(|a| AutomationItem {
                id: a.id,
                name: a.name,
                enabled: a.enabled,
                trigger_type: a.trigger_type,
            })
            .collect(),
    ))
}

async fn list_giveaways(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<GiveawayItem>>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let db = app_state.db.lock();
    let active_id = giveaway::get_active_view(&db)
        .ok()
        .flatten()
        .map(|a| a.giveaway.id);
    let list = db::list_giveaways(&db).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(
        list.into_iter()
            .map(|g| {
                let active = active_id.as_ref() == Some(&g.id);
                GiveawayItem {
                    id: g.id,
                    title: g.title,
                    enabled: g.enabled,
                    active,
                }
            })
            .collect(),
    ))
}

async fn list_media(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MediaItem>>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let clips = db::list_media(&app_state.db.lock()).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(
        clips
            .into_iter()
            .map(|m| MediaItem {
                id: m.id,
                name: m.name,
            })
            .collect(),
    ))
}

async fn run_command(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    engine::run_command_by_id(&state.app, &id).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(OkResponse { ok: true }))
}

async fn run_automation(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    engine::run_automation_by_id(&state.app, &id).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(OkResponse { ok: true }))
}

async fn start_giveaway(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    giveaway::start(&app_state.db.lock(), &id).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let title = db::list_giveaways(&app_state.db.lock())
        .ok()
        .and_then(|list| list.into_iter().find(|g| g.id == id).map(|g| g.title))
        .unwrap_or_else(|| "Giveaway".into());
    crate::activity::push(
        &state.app,
        "giveaway",
        title,
        "Started (Stream Deck)",
        "/giveaways",
        Some(id),
    );
    let _ = state.app.emit("giveaway-updated", ());
    Ok(Json(OkResponse { ok: true }))
}

async fn stop_giveaway(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let active = giveaway::get_active_view(&app_state.db.lock())
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    giveaway::stop_active(&app_state.db.lock()).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    if let Some(a) = active {
        crate::activity::push(
            &state.app,
            "giveaway",
            a.giveaway.title,
            "Stopped (Stream Deck)",
            "/giveaways",
            Some(a.giveaway.id),
        );
    }
    let _ = state.app.emit("giveaway-updated", ());
    Ok(Json(OkResponse { ok: true }))
}

async fn draw_giveaway(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let active = giveaway::get_active_view(&app_state.db.lock())
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let winners =
        giveaway::draw_winners(&app_state.db.lock()).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    if !winners.is_empty() {
        let announce = {
            let db = app_state.db.lock();
            giveaway::format_announce(&db, &winners).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?
        };
        if let Some(tx) = app_state.chat_tx.lock().clone() {
            let _ = tx.send(ChatOutbound::Message(announce));
        }
        if let Some(a) = &active {
            let names = winners
                .iter()
                .map(|w| format!("@{}", w.login))
                .collect::<Vec<_>>()
                .join(", ");
            crate::activity::push(
                &state.app,
                "giveaway",
                a.giveaway.title.clone(),
                format!("Winner(s): {names} (Stream Deck)"),
                "/giveaways",
                Some(a.giveaway.id.clone()),
            );
        }
    }
    let _ = state.app.emit("giveaway-updated", ());
    Ok(Json(OkResponse { ok: true }))
}

async fn play_media(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let app_state = state.app.state::<AppState>();
    let clip = db::get_media(&app_state.db.lock(), &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Media clip not found"))?;
    media::play_clip(&app_state.overlay, &clip);
    crate::activity::push(
        &state.app,
        "media",
        clip.name.clone(),
        format!("Played {} on overlay (Stream Deck)", clip.media_type),
        "/media",
        Some(clip.id),
    );
    Ok(Json(OkResponse { ok: true }))
}

async fn send_chat(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<ChatBody>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    let msg = body.message.trim().to_string();
    if msg.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Message is empty"));
    }
    let app_state = state.app.state::<AppState>();
    let tx = app_state.chat_tx.lock().clone();
    tx.as_ref()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "Bot is not connected."))?
        .send(ChatOutbound::Message(msg.clone()))
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::activity::push(
        &state.app,
        "chat",
        "Bot message",
        format!("{msg} (Stream Deck)"),
        "/",
        None,
    );
    Ok(Json(OkResponse { ok: true }))
}

async fn bot_connect(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    crate::control_connect_bot(state.app.clone())
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(OkResponse { ok: true }))
}

async fn bot_disconnect(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorBody>)> {
    require_auth(&headers, &state.hub)?;
    crate::control_disconnect_bot(state.app.clone()).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(OkResponse { ok: true }))
}

pub async fn start_server(app: AppHandle, hub: Arc<ControlHub>) -> Result<(), String> {
    let port = hub.port;
    let api = ApiState {
        app,
        hub,
    };
    let router = Router::new()
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/commands", get(list_commands))
        .route("/api/v1/automations", get(list_automations))
        .route("/api/v1/giveaways", get(list_giveaways))
        .route("/api/v1/media", get(list_media))
        .route("/api/v1/commands/{id}/run", post(run_command))
        .route("/api/v1/automations/{id}/run", post(run_automation))
        .route("/api/v1/giveaways/{id}/start", post(start_giveaway))
        .route("/api/v1/giveaways/stop", post(stop_giveaway))
        .route("/api/v1/giveaways/draw", post(draw_giveaway))
        .route("/api/v1/media/{id}/play", post(play_media))
        .route("/api/v1/chat", post(send_chat))
        .route("/api/v1/bot/connect", post(bot_connect))
        .route("/api/v1/bot/disconnect", post(bot_disconnect))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(api);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("control API bind {addr}: {e}"))?;
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("control API serve: {e}"))
}
