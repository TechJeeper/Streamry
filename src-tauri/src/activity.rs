use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: String,
    pub at: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    /// Frontend route, e.g. `/commands`
    pub path: String,
    pub entity_id: Option<String>,
}

pub fn push(
    app: &AppHandle,
    kind: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    path: &str,
    entity_id: Option<String>,
) {
    let entry = ActivityEntry {
        id: Uuid::new_v4().to_string(),
        at: Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        title: title.into(),
        detail: detail.into(),
        path: path.to_string(),
        entity_id,
    };
    let _ = app.emit("activity-log", entry);
}
