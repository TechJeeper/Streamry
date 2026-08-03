use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub client_id: String,
    pub channel: String,
    pub bot_login: String,
    pub account_mode: String,
    pub setup_complete: bool,
    pub confirm_giveaway_entry: bool,
    pub timers_live_only: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "dark".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeResponse {
    #[serde(skip_serializing)]
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCommand {
    pub id: String,
    pub name: String,
    pub aliases: String,
    pub response: String,
    pub enabled: bool,
    pub permission: String,
    pub global_cooldown: i64,
    pub user_cooldown: i64,
    #[serde(default)]
    pub media_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaClip {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub file_name: String,
    pub duration_ms: i64,
    pub volume: i64,
    /// Grid placement on 16×9 overlay canvas (0-based).
    #[serde(default = "default_overlay_x")]
    pub overlay_x: i64,
    #[serde(default = "default_overlay_y")]
    pub overlay_y: i64,
    #[serde(default = "default_overlay_w")]
    pub overlay_w: i64,
    #[serde(default = "default_overlay_h")]
    pub overlay_h: i64,
    /// Image-only: keep on overlay until replaced (ignores duration).
    #[serde(default)]
    pub always_show: bool,
    /// Hex color `#RRGGBB` to key out (empty = off). Used for image/gif/video.
    #[serde(default)]
    pub chroma_key: String,
    /// Color distance tolerance for chromakey (0–120).
    #[serde(default = "default_chroma_tolerance")]
    pub chroma_tolerance: i64,
}

pub fn default_overlay_x() -> i64 {
    4
}
pub fn default_overlay_y() -> i64 {
    2
}
pub fn default_overlay_w() -> i64 {
    8
}
pub fn default_overlay_h() -> i64 {
    5
}

pub fn default_chroma_tolerance() -> i64 {
    64
}

pub const OVERLAY_GRID_W: i64 = 16;
pub const OVERLAY_GRID_H: i64 = 9;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayInfo {
    pub url: String,
    pub port: u16,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTimer {
    pub id: String,
    pub name: String,
    pub message: String,
    pub interval_mins: i64,
    pub min_chat_lines: i64,
    pub enabled: bool,
    pub live_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Giveaway {
    pub id: String,
    pub title: String,
    pub prize: String,
    pub entry_command: String,
    pub draw_command: String,
    pub duration_mins: Option<i64>,
    pub winner_count: i64,
    pub eligibility: String,
    pub exclude_mods: bool,
    pub confirm_entry: bool,
    pub announce_template: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GiveawayWinner {
    pub user_id: String,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveGiveawayView {
    pub giveaway: Giveaway,
    pub run_id: String,
    pub status: String,
    pub started_at: String,
    pub ends_at: Option<String>,
    pub entry_count: i64,
    pub winners: Vec<GiveawayWinner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GiveawayRunHistory {
    pub run_id: String,
    pub giveaway_id: String,
    pub title: String,
    pub prize: String,
    pub started_at: String,
    pub ends_at: Option<String>,
    pub entry_count: i64,
    pub winners: Vec<GiveawayWinner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub trigger_type: String,
    pub action_type: String,
    pub action_payload: String,
    pub enabled: bool,
    pub cooldown_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomVariable {
    pub id: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeImportPreview {
    pub commands: Vec<SeCommandPreview>,
    pub timers: Vec<SeTimerPreview>,
    #[serde(default)]
    pub variables: Vec<SeVariablePreview>,
    #[serde(default)]
    pub automations: Vec<SeAutomationPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeCommandPreview {
    pub id: String,
    pub name: String,
    pub response: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeTimerPreview {
    pub id: String,
    pub name: String,
    pub message: String,
    pub interval: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeVariablePreview {
    pub id: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeAutomationPreview {
    pub id: String,
    pub name: String,
    pub trigger_type: String,
    pub action_payload: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_commands: usize,
    pub imported_timers: usize,
    pub imported_giveaways: usize,
    pub imported_automations: usize,
    #[serde(default)]
    pub imported_variables: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub commands: usize,
    pub timers: usize,
    pub giveaways: usize,
    pub automations: usize,
    #[serde(default)]
    pub variables: usize,
    pub exported_at: String,
}

#[derive(Debug, Clone)]
pub struct IncomingChat {
    pub user_id: String,
    pub login: String,
    pub display: String,
    pub message: String,
    pub is_mod: bool,
    pub is_vip: bool,
    pub is_sub: bool,
    pub is_broadcaster: bool,
}
