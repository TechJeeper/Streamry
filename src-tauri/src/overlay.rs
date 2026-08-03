use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

pub const DEFAULT_PORT: u16 = 1919;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayEvent {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub url: String,
    pub duration_ms: i64,
    pub volume: f64,
}

#[derive(Clone)]
pub struct OverlayHub {
    tx: broadcast::Sender<String>,
    media_dir: PathBuf,
    port: u16,
}

impl OverlayHub {
    pub fn new(media_dir: PathBuf, port: u16) -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            media_dir,
            port,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn browser_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    pub fn media_dir(&self) -> &PathBuf {
        &self.media_dir
    }

    pub fn publish(&self, event: OverlayEvent) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.tx.send(json);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}

pub async fn start_server(hub: Arc<OverlayHub>) -> Result<(), String> {
    let state = hub.clone();
    let app = Router::new()
        .route("/", get(overlay_page))
        .route("/ws", get(ws_handler))
        .route("/media/{id}", get(serve_media))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], hub.port()));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Overlay server bind failed on {addr}: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Overlay server error: {e}"))?;
    Ok(())
}

async fn overlay_page() -> Html<&'static str> {
    Html(OVERLAY_HTML)
}

async fn ws_handler(ws: WebSocketUpgrade, State(hub): State<Arc<OverlayHub>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, hub))
}

async fn handle_socket(socket: WebSocket, hub: Arc<OverlayHub>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = hub.subscribe();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Keep reading until client disconnects (ignore inbound messages)
    while let Some(Ok(_)) = receiver.next().await {}
    send_task.abort();
}

async fn serve_media(
    Path(id): Path<String>,
    State(hub): State<Arc<OverlayHub>>,
) -> Response {
    let safe = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !safe || id.contains("..") {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }

    let path = hub.media_dir.join(&id);
    if !path.is_file() {
        // Try matching by id prefix (file stored as {uuid}{ext})
        if let Ok(entries) = std::fs::read_dir(&hub.media_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == id || name.starts_with(&format!("{id}.")) {
                    return file_response(entry.path());
                }
            }
        }
        return (StatusCode::NOT_FOUND, "media not found").into_response();
    }
    file_response(path)
}

fn file_response(path: PathBuf) -> Response {
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return (StatusCode::NOT_FOUND, "read failed").into_response(),
    };
    let mime = mime_for_path(&path);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")],
        bytes,
    )
        .into_response()
}

fn mime_for_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "webm" => "video/webm",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "gif" => "image/gif",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

const OVERLAY_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Streamry Overlay</title>
<style>
  html, body {
    margin: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: transparent !important;
    font-family: system-ui, sans-serif;
  }
  #stage {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  #stage img, #stage video {
    max-width: min(90vw, 960px);
    max-height: min(80vh, 720px);
    object-fit: contain;
    opacity: 0;
    transform: scale(0.92);
    transition: opacity 0.25s ease, transform 0.25s ease;
  }
  #stage img.show, #stage video.show {
    opacity: 1;
    transform: scale(1);
  }
  #hint {
    position: fixed;
    left: 12px;
    bottom: 12px;
    color: rgba(255,255,255,0.35);
    font-size: 12px;
    opacity: 0;
    transition: opacity 0.4s ease;
  }
  body.show-hint #hint { opacity: 1; }
</style>
</head>
<body>
  <div id="stage"></div>
  <div id="hint">Streamry overlay connected</div>
  <script>
    const stage = document.getElementById('stage');
    let hideTimer = null;
    let audioEl = null;

    function clearStage() {
      stage.innerHTML = '';
      if (audioEl) {
        audioEl.pause();
        audioEl = null;
      }
      if (hideTimer) {
        clearTimeout(hideTimer);
        hideTimer = null;
      }
    }

    function play(evt) {
      clearStage();
      const vol = Math.max(0, Math.min(1, Number(evt.volume) || 1));
      const duration = Math.max(500, Number(evt.durationMs) || 5000);
      const type = (evt.mediaType || '').toLowerCase();
      const url = evt.url;

      if (type === 'sound') {
        audioEl = new Audio(url);
        audioEl.volume = vol;
        audioEl.play().catch(() => {});
        hideTimer = setTimeout(clearStage, duration);
        return;
      }

      if (type === 'video') {
        const v = document.createElement('video');
        v.src = url;
        v.autoplay = true;
        v.playsInline = true;
        v.volume = vol;
        v.className = 'show';
        stage.appendChild(v);
        v.play().catch(() => {});
        const end = () => clearStage();
        v.onended = end;
        hideTimer = setTimeout(end, duration);
        return;
      }

      // image / gif
      const img = document.createElement('img');
      img.src = url;
      img.alt = evt.name || '';
      stage.appendChild(img);
      requestAnimationFrame(() => img.classList.add('show'));
      if (type === 'sound' || type === 'image' || type === 'gif') {
        // optional companion audio not used for image/gif
      }
      hideTimer = setTimeout(() => {
        img.classList.remove('show');
        setTimeout(clearStage, 280);
      }, duration);
    }

    function connect() {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws';
      const ws = new WebSocket(proto + '://' + location.host + '/ws');
      ws.onopen = () => {
        document.body.classList.add('show-hint');
        setTimeout(() => document.body.classList.remove('show-hint'), 2500);
      };
      ws.onmessage = (e) => {
        try { play(JSON.parse(e.data)); } catch (_) {}
      };
      ws.onclose = () => setTimeout(connect, 1500);
      ws.onerror = () => ws.close();
    }
    connect();
  </script>
</body>
</html>
"#;
