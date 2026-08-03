use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::models::MediaClip;
use crate::overlay::{OverlayEvent, OverlayHub};

pub fn media_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Streamry")
        .join("media");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn detect_media_type(path: &Path) -> Result<&'static str, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "ogg" | "m4a" => Ok("sound"),
        "gif" => Ok("gif"),
        "png" | "jpg" | "jpeg" | "webp" | "svg" => Ok("image"),
        "mp4" | "webm" | "mov" => Ok("video"),
        _ => Err(format!(
            "Unsupported file type .{ext}. Use sound (mp3/wav/ogg/m4a), image (png/jpg/webp), gif, or video (mp4/webm/mov)."
        )),
    }
}

pub fn default_duration_ms(media_type: &str) -> i64 {
    match media_type {
        "sound" => 8000,
        "video" => 15000,
        "gif" => 6000,
        _ => 5000,
    }
}

pub fn import_file(source: &Path, name: Option<String>) -> Result<(MediaClip, PathBuf), String> {
    if !source.is_file() {
        return Err("File not found".into());
    }
    let media_type = detect_media_type(source)?;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_lowercase();
    let id = Uuid::new_v4().to_string();
    let file_name = format!("{id}.{ext}");
    let dest = media_dir().join(&file_name);
    std::fs::copy(source, &dest).map_err(|e| format!("Copy failed: {e}"))?;

    let display_name = name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            source
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Media".into())
        });

    let clip = MediaClip {
        id,
        name: display_name,
        media_type: media_type.to_string(),
        file_name,
        duration_ms: default_duration_ms(media_type),
        volume: 80,
        overlay_x: crate::models::default_overlay_x(),
        overlay_y: crate::models::default_overlay_y(),
        overlay_w: crate::models::default_overlay_w(),
        overlay_h: crate::models::default_overlay_h(),
        always_show: false,
        chroma_key: String::new(),
        chroma_tolerance: crate::models::default_chroma_tolerance(),
    };
    Ok((clip, dest))
}

pub fn delete_file(file_name: &str) {
    if file_name.is_empty() || file_name.contains("..") || file_name.contains('/') || file_name.contains('\\')
    {
        return;
    }
    let path = media_dir().join(file_name);
    let _ = std::fs::remove_file(path);
}

pub fn play_clip(hub: &OverlayHub, clip: &MediaClip) {
    let volume = (clip.volume as f64 / 100.0).clamp(0.0, 1.0);
    let always_show = clip.always_show && clip.media_type == "image";
    let chroma = matches!(clip.media_type.as_str(), "image" | "gif" | "video")
        .then(|| clip.chroma_key.clone())
        .unwrap_or_default();
    hub.publish(OverlayEvent {
        id: clip.id.clone(),
        name: clip.name.clone(),
        media_type: clip.media_type.clone(),
        url: format!("/media/{}", clip.file_name),
        duration_ms: clip.duration_ms.max(500),
        volume,
        grid_x: clip.overlay_x,
        grid_y: clip.overlay_y,
        grid_w: clip.overlay_w,
        grid_h: clip.overlay_h,
        always_show,
        chroma_key: chroma,
        chroma_tolerance: clip.chroma_tolerance.clamp(0, 120),
    });
}
