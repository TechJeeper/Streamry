use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::control::DEFAULT_PORT;
use crate::db;
use crate::models::StreamDeckStatus;
use crate::AppState;

pub const PLUGIN_FOLDER_NAME: &str = "com.streamry.streamdeck.sdPlugin";
pub const SETTINGS_FILE: &str = "streamry-connection.json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginConnection {
    base_url: String,
    token: String,
    port: u16,
}

pub fn plugins_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|p| p.join("Elgato").join("StreamDeck").join("Plugins"))
    }
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|p| {
            p.join("Library")
                .join("Application Support")
                .join("com.elgato.StreamDeck")
                .join("Plugins")
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

pub fn install_path() -> Option<PathBuf> {
    plugins_dir().map(|d| d.join(PLUGIN_FOLDER_NAME))
}

pub fn is_installed() -> bool {
    install_path()
        .map(|p| p.join("manifest.json").is_file())
        .unwrap_or(false)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry.path(), &to).map_err(|e| format!("copy {:?}: {e}", entry.path()))?;
        }
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| format!("remove {:?}: {e}", path))?;
    }
    Ok(())
}

fn bundled_plugin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // Dev: repo resources next to src-tauri
    let resource = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("streamdeck")
        .join(PLUGIN_FOLDER_NAME);
    if resource.join("manifest.json").is_file() {
        return Ok(resource);
    }

    // Fallback: cwd-relative (dev without bundled resources)
    let candidates = [
        PathBuf::from("resources")
            .join("streamdeck")
            .join(PLUGIN_FOLDER_NAME),
        PathBuf::from("src-tauri")
            .join("resources")
            .join("streamdeck")
            .join(PLUGIN_FOLDER_NAME),
        PathBuf::from("..")
            .join("src-tauri")
            .join("resources")
            .join("streamdeck")
            .join(PLUGIN_FOLDER_NAME),
    ];
    for c in candidates {
        if c.join("manifest.json").is_file() {
            return Ok(c);
        }
    }
    Err(
        "Bundled Stream Deck plugin not found. Rebuild the app after building the streamdeck package."
            .into(),
    )
}

pub fn ensure_token(state: &AppState) -> Result<String, String> {
    let existing = {
        let db = state.db.lock();
        db::get_setting(&db, "stream_deck_token")?.unwrap_or_default()
    };
    if !existing.is_empty() {
        state.control.set_token(existing.clone());
        return Ok(existing);
    }
    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    {
        let db = state.db.lock();
        db::set_setting(&db, "stream_deck_token", &token)?;
        db::set_setting(&db, "stream_deck_control_enabled", "1")?;
        let port = db::get_setting(&db, "stream_deck_control_port")?
            .unwrap_or_else(|| DEFAULT_PORT.to_string());
        if port.is_empty() {
            db::set_setting(&db, "stream_deck_control_port", &DEFAULT_PORT.to_string())?;
        }
    }
    state.control.set_token(token.clone());
    state.control.set_enabled(true);
    Ok(token)
}

pub fn set_control_enabled(state: &AppState, enabled: bool) -> Result<StreamDeckStatus, String> {
    if enabled {
        let token = ensure_token(state)?;
        {
            let db = state.db.lock();
            db::set_setting(&db, "stream_deck_control_enabled", "1")?;
        }
        state.control.set_enabled(true);
        if is_installed() {
            let port = {
                let db = state.db.lock();
                db::get_setting(&db, "stream_deck_control_port")?
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_PORT)
            };
            let _ = write_connection_file(port, &token);
        }
    } else {
        {
            let db = state.db.lock();
            db::set_setting(&db, "stream_deck_control_enabled", "0")?;
        }
        state.control.set_enabled(false);
    }
    Ok(status(state))
}

pub fn write_connection_file(port: u16, token: &str) -> Result<PathBuf, String> {
    let dest = install_path().ok_or_else(|| {
        "Stream Deck is not supported on this OS (Windows and macOS only).".to_string()
    })?;
    if !dest.exists() {
        return Err("Plugin is not installed yet.".into());
    }
    let conn = PluginConnection {
        base_url: format!("http://127.0.0.1:{port}"),
        token: token.to_string(),
        port,
    };
    let path = dest.join(SETTINGS_FILE);
    let json = serde_json::to_string_pretty(&conn).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn install_plugin(app: &AppHandle, state: &AppState) -> Result<StreamDeckStatus, String> {
    let plugins = plugins_dir().ok_or_else(|| {
        "Stream Deck plugin install is only supported on Windows and macOS.".to_string()
    })?;
    fs::create_dir_all(&plugins).map_err(|e| format!("Create Plugins folder: {e}"))?;

    let src = bundled_plugin_dir(app)?;
    let dest = plugins.join(PLUGIN_FOLDER_NAME);
    remove_dir_if_exists(&dest)?;
    copy_dir_recursive(&src, &dest)?;

    let token = ensure_token(state)?;
    let port = {
        let db = state.db.lock();
        db::get_setting(&db, "stream_deck_control_port")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT)
    };
    // Ensure enabled
    {
        let db = state.db.lock();
        db::set_setting(&db, "stream_deck_control_enabled", "1")?;
    }
    state.control.set_enabled(true);
    state.control.set_token(token.clone());
    write_connection_file(port, &token)?;

    Ok(status(state))
}

pub fn status(state: &AppState) -> StreamDeckStatus {
    let supported = plugins_dir().is_some();
    let path = install_path();
    let installed = is_installed();
    let (enabled, port, has_token) = {
        let db = state.db.lock();
        let enabled = db::get_setting(&db, "stream_deck_control_enabled")
            .ok()
            .flatten()
            .as_deref()
            == Some("1");
        let port = db::get_setting(&db, "stream_deck_control_port")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        let has_token = db::get_setting(&db, "stream_deck_token")
            .ok()
            .flatten()
            .map(|t| !t.is_empty())
            .unwrap_or(false);
        (enabled, port, has_token)
    };

    let message = if !supported {
        "Stream Deck integration is available on Windows and macOS.".into()
    } else if !installed {
        "Plugin not installed. Click Install to add Streamry actions to Stream Deck.".into()
    } else if !enabled {
        "Plugin installed. Enable the control API so Stream Deck can reach Streamry.".into()
    } else {
        "Installed. Restart Stream Deck if actions don’t appear, then find Streamry in the action list.".into()
    };

    StreamDeckStatus {
        installed,
        install_path: path.map(|p| p.to_string_lossy().into_owned()),
        control_enabled: enabled,
        control_port: port,
        control_running: state.control.is_enabled(),
        has_token,
        supported,
        message,
    }
}
