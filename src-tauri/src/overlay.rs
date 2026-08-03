use std::io::{Read, Seek, SeekFrom};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
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
    pub grid_x: i64,
    pub grid_y: i64,
    pub grid_w: i64,
    pub grid_h: i64,
    pub always_show: bool,
    pub chroma_key: String,
    pub chroma_tolerance: i64,
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
    headers: HeaderMap,
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
                    return file_response(entry.path(), &headers);
                }
            }
        }
        return (StatusCode::NOT_FOUND, "media not found").into_response();
    }
    file_response(path, &headers)
}

/// Parse `Range: bytes=start-end` (end optional). Returns inclusive start/end.
fn parse_byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let s = header.strip_prefix("bytes=")?.trim();
    // Only single ranges — multiparts are rare for <video>
    let s = s.split(',').next()?.trim();
    let (start_s, end_s) = s.split_once('-')?;
    if start_s.is_empty() {
        // suffix: bytes=-N
        let n: u64 = end_s.parse().ok()?;
        if n == 0 || len == 0 {
            return None;
        }
        let n = n.min(len);
        return Some((len - n, len - 1));
    }
    let start: u64 = start_s.parse().ok()?;
    if start >= len {
        return None;
    }
    let end = if end_s.is_empty() {
        len - 1
    } else {
        end_s.parse::<u64>().ok()?.min(len - 1)
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

fn file_response(path: PathBuf, headers: &HeaderMap) -> Response {
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "read failed").into_response(),
    };
    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return (StatusCode::NOT_FOUND, "read failed").into_response(),
    };
    let mime = mime_for_path(&path);

    let mut res_headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(mime) {
        res_headers.insert(header::CONTENT_TYPE, v);
    }
    res_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    res_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));

    if let Some(range_hv) = headers.get(header::RANGE) {
        if let Ok(range_str) = range_hv.to_str() {
            match parse_byte_range(range_str, len) {
                Some((start, end)) => {
                    let content_len = end - start + 1;
                    let mut buf = vec![0u8; content_len as usize];
                    if file.seek(SeekFrom::Start(start)).is_err()
                        || file.read_exact(&mut buf).is_err()
                    {
                        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                    }
                    if let Ok(v) = HeaderValue::from_str(&content_len.to_string()) {
                        res_headers.insert(header::CONTENT_LENGTH, v);
                    }
                    if let Ok(v) =
                        HeaderValue::from_str(&format!("bytes {start}-{end}/{len}"))
                    {
                        res_headers.insert(header::CONTENT_RANGE, v);
                    }
                    return (StatusCode::PARTIAL_CONTENT, res_headers, buf).into_response();
                }
                None if range_str.trim_start().starts_with("bytes=") => {
                    if let Ok(v) = HeaderValue::from_str(&format!("bytes */{len}")) {
                        res_headers.insert(header::CONTENT_RANGE, v);
                    }
                    return (StatusCode::RANGE_NOT_SATISFIABLE, res_headers).into_response();
                }
                None => {}
            }
        }
    }

    let mut bytes = Vec::with_capacity(len as usize);
    if file.read_to_end(&mut bytes).is_err() {
        return (StatusCode::NOT_FOUND, "read failed").into_response();
    }
    if let Ok(v) = HeaderValue::from_str(&len.to_string()) {
        res_headers.insert(header::CONTENT_LENGTH, v);
    }
    (StatusCode::OK, res_headers, bytes).into_response()
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
  #persist, #stage {
    position: fixed;
    inset: 0;
    pointer-events: none;
  }
  #persist { z-index: 1; }
  #stage { z-index: 2; }
  .slot {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    overflow: hidden;
  }
  .slot img, .slot video, .slot canvas {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  /* Keep decoder alive off-screen; tiny CSS boxes can stall video frames. */
  .slot .chroma-src {
    position: absolute;
    left: -10000px;
    top: 0;
    width: auto;
    height: auto;
    max-width: none;
    max-height: none;
    opacity: 0;
    pointer-events: none;
  }
  #stage img, #stage video, #stage canvas {
    opacity: 0;
    transform: scale(0.96);
    transition: opacity 0.25s ease, transform 0.25s ease;
  }
  #stage img.show, #stage video.show, #stage canvas.show {
    opacity: 1;
    transform: scale(1);
  }
  #persist img, #persist canvas { opacity: 1; }
  #hint {
    position: fixed;
    left: 12px;
    bottom: 12px;
    z-index: 3;
    color: rgba(255,255,255,0.35);
    font-size: 12px;
    opacity: 0;
    transition: opacity 0.4s ease;
  }
  body.show-hint #hint { opacity: 1; }
</style>
</head>
<body>
  <div id="persist"></div>
  <div id="stage"></div>
  <div id="hint">Streamry overlay connected</div>
  <script>
    const persist = document.getElementById('persist');
    const stage = document.getElementById('stage');
    let instanceSeq = 0;
    const GRID_W = 16;
    const GRID_H = 9;

    function clampRect(evt) {
      let w = Math.max(1, Math.min(GRID_W, Number(evt.gridW) || 8));
      let h = Math.max(1, Math.min(GRID_H, Number(evt.gridH) || 5));
      let x = Math.max(0, Math.min(GRID_W - w, Number(evt.gridX) || 0));
      let y = Math.max(0, Math.min(GRID_H - h, Number(evt.gridY) || 0));
      return { x, y, w, h };
    }

    function applyGrid(slot, evt) {
      const r = clampRect(evt);
      slot.style.left = (r.x / GRID_W * 100) + '%';
      slot.style.top = (r.y / GRID_H * 100) + '%';
      slot.style.width = (r.w / GRID_W * 100) + '%';
      slot.style.height = (r.h / GRID_H * 100) + '%';
    }

    function parseChromaKey(hex) {
      if (!hex || typeof hex !== 'string') return null;
      const m = String(hex).trim().match(/^#?([0-9a-fA-F]{6})$/);
      if (!m) return null;
      const n = parseInt(m[1], 16);
      const r = (n >> 16) & 255;
      const g = (n >> 8) & 255;
      const b = n & 255;
      // Precompute YCbCr chroma channels for robust greenscreen-style keying
      return {
        r, g, b,
        cb: 128 - 0.168736 * r - 0.331264 * g + 0.5 * b,
        cr: 128 + 0.5 * r - 0.418688 * g - 0.081312 * b,
      };
    }

    function keyFrame(ctx, w, h, key, tol) {
      let imageData;
      try {
        imageData = ctx.getImageData(0, 0, w, h);
      } catch (_) {
        // Canvas tainted / not readable — leave frame as-is
        return false;
      }
      const d = imageData.data;
      // UI tolerance 0–120 → CbCr distance; soft edge for less fringe
      const hard = Math.max(4, tol * 0.55);
      const soft = hard * 1.55;
      for (let i = 0; i < d.length; i += 4) {
        const r = d[i], g = d[i + 1], b = d[i + 2];
        const cb = 128 - 0.168736 * r - 0.331264 * g + 0.5 * b;
        const cr = 128 + 0.5 * r - 0.418688 * g - 0.081312 * b;
        const dist = Math.hypot(cb - key.cb, cr - key.cr);
        if (dist <= hard) {
          d[i + 3] = 0;
        } else if (dist < soft) {
          d[i + 3] = Math.round(d[i + 3] * ((dist - hard) / (soft - hard)));
        }
      }
      ctx.putImageData(imageData, 0, 0);
      return true;
    }

    /** Returns { el, stop } — el is the visible element (canvas or source). */
    function attachVisual(slot, source, evt, animated) {
      const key = parseChromaKey(evt.chromaKey);
      const tol = Math.max(0, Math.min(120, Number(evt.chromaTolerance) || 48));
      if (!key) {
        slot.appendChild(source);
        return { el: source, stop: () => {} };
      }
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      source.classList.add('chroma-src');
      // Do NOT set crossOrigin — same-origin media; wrong CORS mode taints the canvas.
      slot.appendChild(source);
      slot.appendChild(canvas);
      let raf = 0;
      let stopped = false;
      const paint = () => {
        if (stopped || !ctx) return;
        const iw = source.videoWidth || source.naturalWidth || 0;
        const ih = source.videoHeight || source.naturalHeight || 0;
        if (iw > 0 && ih > 0) {
          // Cap working size so keying stays realtime on large clips
          const maxW = 1280;
          let w = iw, h = ih;
          if (w > maxW) {
            h = Math.round(h * (maxW / w));
            w = maxW;
          }
          if (canvas.width !== w || canvas.height !== h) {
            canvas.width = w;
            canvas.height = h;
          }
          ctx.drawImage(source, 0, 0, w, h);
          keyFrame(ctx, w, h, key, tol);
        }
        if (animated) raf = requestAnimationFrame(paint);
      };
      if (animated) {
        paint();
      } else {
        const once = () => {
          paint();
          // Retry a couple times in case decode wasn't ready on first load
          requestAnimationFrame(paint);
        };
        if (source.complete && source.naturalWidth) once();
        else source.addEventListener('load', once, { once: true });
      }
      return {
        el: canvas,
        stop: () => {
          stopped = true;
          if (raf) cancelAnimationFrame(raf);
        },
      };
    }

    function showPersist(evt) {
      const id = 'persist-' + String(evt.id || 'anon');
      let slot = document.getElementById(id);
      if (!slot) {
        slot = document.createElement('div');
        slot.id = id;
        slot.className = 'slot';
        persist.appendChild(slot);
      }
      if (slot._chromaStop) {
        slot._chromaStop();
        slot._chromaStop = null;
      }
      applyGrid(slot, evt);
      slot.innerHTML = '';
      const img = document.createElement('img');
      img.alt = evt.name || '';
      img.src = evt.url;
      const { el, stop } = attachVisual(slot, img, evt, false);
      slot._chromaStop = stop;
      el.classList.add('show');
    }

    function removeSlot(slot, mediaEl) {
      if (!slot || !slot.isConnected) return;
      if (slot._timer) {
        clearTimeout(slot._timer);
        slot._timer = null;
      }
      if (slot._chromaStop) {
        slot._chromaStop();
        slot._chromaStop = null;
      }
      if (mediaEl) {
        mediaEl.classList.remove('show');
        setTimeout(() => slot.remove(), 280);
      } else {
        slot.remove();
      }
    }

    function play(evt) {
      const vol = Math.max(0, Math.min(1, Number(evt.volume) || 1));
      const duration = Math.max(500, Number(evt.durationMs) || 5000);
      const type = (evt.mediaType || '').toLowerCase();
      const url = evt.url;
      const instanceId = 'm-' + (++instanceSeq) + '-' + Date.now();

      if (type === 'image' && evt.alwaysShow) {
        showPersist(evt);
        return;
      }

      if (type === 'image' && evt.id) {
        document.getElementById('persist-' + evt.id)?.remove();
      }

      if (type === 'sound') {
        const audio = new Audio(url);
        audio.volume = vol;
        audio.play().catch(() => {});
        const stop = () => {
          try { audio.pause(); } catch (_) {}
        };
        audio.onended = stop;
        setTimeout(stop, duration);
        return;
      }

      const slot = document.createElement('div');
      slot.id = instanceId;
      slot.className = 'slot';
      applyGrid(slot, evt);
      stage.appendChild(slot);

      if (type === 'video') {
        const v = document.createElement('video');
        v.preload = 'auto';
        v.autoplay = true;
        v.playsInline = true;
        v.setAttribute('playsinline', '');
        v.setAttribute('webkit-playsinline', '');
        v.muted = true;
        v.volume = vol;
        v.src = url;
        const keyOn = !!parseChromaKey(evt.chromaKey);
        const keyed = attachVisual(slot, v, evt, keyOn);
        slot._chromaStop = keyed.stop;
        const reveal = () => keyed.el.classList.add('show');
        v.addEventListener('loadeddata', reveal, { once: true });
        v.addEventListener('playing', reveal, { once: true });
        const tryPlay = () => {
          const p = v.play();
          if (!p) return;
          p.then(() => {
            if (vol > 0) {
              v.muted = false;
              v.volume = vol;
            }
            reveal();
          }).catch(() => {
            v.muted = true;
            v.play().then(reveal).catch(() => reveal());
          });
        };
        if (v.readyState >= 2) tryPlay();
        else v.addEventListener('canplay', tryPlay, { once: true });
        const end = () => {
          try { v.pause(); } catch (_) {}
          removeSlot(slot, keyed.el);
        };
        v.onended = end;
        v.onerror = () => removeSlot(slot, null);
        slot._timer = setTimeout(end, duration);
        return;
      }

      // Timed image / gif
      const img = document.createElement('img');
      img.alt = evt.name || '';
      img.src = url;
      const keyOn = !!parseChromaKey(evt.chromaKey);
      const keyed = attachVisual(slot, img, evt, keyOn && type === 'gif');
      slot._chromaStop = keyed.stop;
      const reveal = () => keyed.el.classList.add('show');
      if (img.complete) requestAnimationFrame(reveal);
      else img.addEventListener('load', () => requestAnimationFrame(reveal), { once: true });
      slot._timer = setTimeout(() => removeSlot(slot, keyed.el), duration);
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
