use std::collections::HashSet;
use std::fs::File;
use std::io::Read;

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use zip::ZipArchive;

use crate::db;
use crate::models::*;

#[derive(Debug, Deserialize)]
struct SeCommand {
    #[serde(default, rename = "_id")]
    id: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    reply: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    cooldown: SeCooldown,
    #[serde(default = "default_access")]
    #[serde(rename = "accessLevel")]
    access_level: i64,
}

#[derive(Debug, Deserialize, Default)]
struct SeCooldown {
    #[serde(default)]
    user: i64,
    #[serde(default)]
    global: i64,
}

#[derive(Debug, Deserialize, Default)]
struct SeTimerSchedule {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct SeTimer {
    #[serde(default, rename = "_id")]
    id: String,
    #[serde(default)]
    name: String,
    /// Older / docs-shaped flat fields.
    #[serde(default)]
    message: String,
    #[serde(default)]
    interval: i64,
    /// StreamElements API / export.stream shape.
    #[serde(default)]
    messages: Vec<String>,
    #[serde(default)]
    online: SeTimerSchedule,
    #[serde(default)]
    offline: SeTimerSchedule,
    #[serde(default)]
    #[serde(rename = "chatLines")]
    chat_lines: i64,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct SeVariable {
    #[serde(default, rename = "_id")]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
}

fn default_true() -> bool {
    true
}
fn default_access() -> i64 {
    100
}

fn cmd_key(c: &SeCommand) -> String {
    if c.id.is_empty() {
        c.command.clone()
    } else {
        c.id.clone()
    }
}

fn timer_key(t: &SeTimer) -> String {
    if t.id.is_empty() {
        format!("{}:{}", t.name, se_timer_interval_mins(t))
    } else {
        t.id.clone()
    }
}

fn se_timer_message(t: &SeTimer) -> String {
    let flat = t.message.trim();
    if !flat.is_empty() {
        return normalize_se_message(flat);
    }
    let parts: Vec<String> = t
        .messages
        .iter()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(normalize_se_message)
        .collect();
    parts.join(" | ")
}

fn se_timer_interval_mins(t: &SeTimer) -> i64 {
    if t.interval > 0 {
        return t.interval;
    }
    if t.online.interval > 0 {
        return t.online.interval;
    }
    if t.offline.interval > 0 {
        return t.offline.interval;
    }
    10
}

fn se_timer_live_only(t: &SeTimer) -> bool {
    t.online.enabled && !t.offline.enabled
}

fn var_key(v: &SeVariable) -> String {
    if v.id.is_empty() {
        v.name.clone()
    } else {
        v.id.clone()
    }
}

fn read_zip_file(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<String>, String> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let path = file.name().replace('\\', "/");
        if path.ends_with(name) {
            let mut buf = String::new();
            file.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            return Ok(Some(buf));
        }
    }
    Ok(None)
}

fn list_zip_json_paths(archive: &mut ZipArchive<File>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let path = file.name().replace('\\', "/").to_lowercase();
        if path.ends_with(".json") && !path.contains("__macosx") {
            out.push(file.name().replace('\\', "/"));
        }
    }
    Ok(out)
}

fn normalize_se_message(msg: &str) -> String {
    msg.replace("{user}", "${user}")
        .replace("{name}", "${user}")
        .replace("{username}", "${user}")
        .replace("$(user)", "${user}")
        .replace("$(name)", "${user}")
}

fn map_se_trigger(raw: &str) -> Option<&'static str> {
    let key = raw.to_lowercase().replace(['-', '_', ' '], "");
    match key.as_str() {
        "subscriber" | "subscribers" | "subscription" | "subscriptions" | "sub" | "subs"
        | "resub" | "resubscribe" | "gift" | "gifted" | "subgift" => Some("subscribe"),
        "raid" | "raids" => Some("raid"),
        "cheer" | "cheers" | "bits" | "cheerbits" => Some("cheer"),
        "streamonline" | "online" | "live" | "golive" | "liveannouncement" => Some("stream_online"),
        "streamoffline" | "offline" | "endstream" => Some("stream_offline"),
        _ => None,
    }
}

fn first_message(val: &Value) -> Option<String> {
    if let Some(s) = val.as_str() {
        let t = s.trim();
        if !t.is_empty() {
            return Some(normalize_se_message(t));
        }
    }
    if let Some(arr) = val.as_array() {
        for item in arr {
            if let Some(s) = item.as_str() {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(normalize_se_message(t));
                }
            } else if let Some(obj) = item.as_object() {
                for key in ["message", "msg", "text", "reply"] {
                    if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
                        let t = s.trim();
                        if !t.is_empty() {
                            return Some(normalize_se_message(t));
                        }
                    }
                }
            }
        }
    }
    if let Some(obj) = val.as_object() {
        for key in ["message", "msg", "text", "reply"] {
            if let Some(found) = obj.get(key).and_then(first_message) {
                return Some(found);
            }
        }
        if let Some(found) = obj.get("messages").and_then(first_message) {
            return Some(found);
        }
    }
    None
}

fn event_enabled(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Object(obj) => obj
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        _ => true,
    }
}

fn push_automation_preview(
    out: &mut Vec<SeAutomationPreview>,
    seen: &mut HashSet<String>,
    trigger_raw: &str,
    payload_src: &Value,
    enabled: bool,
) {
    let Some(trigger) = map_se_trigger(trigger_raw) else {
        return;
    };
    let Some(message) = first_message(payload_src) else {
        return;
    };
    let id = format!("se-alert:{trigger}");
    if !seen.insert(id.clone()) {
        return;
    }
    out.push(SeAutomationPreview {
        id,
        name: format!("SE {trigger_raw}"),
        trigger_type: trigger.to_string(),
        action_payload: message,
        enabled,
    });
}

fn extract_automations_from_value(val: &Value, out: &mut Vec<SeAutomationPreview>) {
    let mut seen: HashSet<String> = out.iter().map(|a| a.id.clone()).collect();

    match val {
        Value::Array(items) => {
            for item in items {
                extract_automations_from_value(item, out);
            }
        }
        Value::Object(obj) => {
            let type_hint = obj
                .get("type")
                .or_else(|| obj.get("name"))
                .or_else(|| obj.get("module"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();

            let looks_like_alerts = type_hint.contains("alert")
                || type_hint.contains("chatalert")
                || obj.contains_key("subscriber")
                || obj.contains_key("raid")
                || obj.contains_key("cheer")
                || obj
                    .get("options")
                    .and_then(|o| o.as_object())
                    .map(|o| {
                        o.contains_key("subscriber")
                            || o.contains_key("raid")
                            || o.contains_key("cheer")
                    })
                    .unwrap_or(false)
                || obj
                    .get("config")
                    .and_then(|o| o.as_object())
                    .map(|o| {
                        o.contains_key("subscriber")
                            || o.contains_key("raid")
                            || o.contains_key("cheer")
                    })
                    .unwrap_or(false);

            if looks_like_alerts {
                let module_enabled = obj
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let buckets = ["options", "config", "settings", "events"];
                let mut scanned = false;
                for bucket in buckets {
                    if let Some(inner) = obj.get(bucket).and_then(|v| v.as_object()) {
                        scanned = true;
                        for (key, event_val) in inner {
                            push_automation_preview(
                                out,
                                &mut seen,
                                key,
                                event_val,
                                module_enabled && event_enabled(event_val),
                            );
                        }
                    }
                }
                if !scanned {
                    for (key, event_val) in obj {
                        if matches!(
                            key.as_str(),
                            "type" | "name" | "module" | "_id" | "id" | "enabled"
                        ) {
                            continue;
                        }
                        push_automation_preview(
                            out,
                            &mut seen,
                            key,
                            event_val,
                            module_enabled && event_enabled(event_val),
                        );
                    }
                }
            }

            // Nested modules arrays / objects
            for key in ["modules", "data", "items", "alerts", "chatAlerts", "chatalerts"] {
                if let Some(child) = obj.get(key) {
                    extract_automations_from_value(child, out);
                }
            }
        }
        _ => {}
    }
}

fn parse_automations_from_archive(
    archive: &mut ZipArchive<File>,
) -> Result<Vec<SeAutomationPreview>, String> {
    let mut out = Vec::new();
    let preferred = [
        "chat-alerts.json",
        "chatalerts.json",
        "chat_alerts.json",
        "modules.json",
        "bot-modules.json",
    ];
    for name in preferred {
        if let Some(json) = read_zip_file(archive, name)? {
            if let Ok(val) = serde_json::from_str::<Value>(&json) {
                extract_automations_from_value(&val, &mut out);
            }
        }
    }

    // Broader scan for module / alert JSON left in the ZIP
    let paths = list_zip_json_paths(archive)?;
    for path in paths {
        let lower = path.to_lowercase();
        if !(lower.contains("alert") || lower.contains("module") || lower.contains("chatalert")) {
            continue;
        }
        if preferred.iter().any(|p| lower.ends_with(p)) {
            continue;
        }
        if let Some(json) = read_zip_file(archive, &path)? {
            if let Ok(val) = serde_json::from_str::<Value>(&json) {
                extract_automations_from_value(&val, &mut out);
            }
        }
    }
    Ok(out)
}

pub fn parse_se_zip(path: &str) -> Result<SeImportPreview, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut commands = Vec::new();
    if let Some(json) = read_zip_file(&mut archive, "custom-commands.json")? {
        let parsed: Vec<SeCommand> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for c in parsed {
            commands.push(SeCommandPreview {
                id: cmd_key(&c),
                name: c.command,
                response: c.reply,
                enabled: c.enabled,
            });
        }
    }

    let mut timers = Vec::new();
    if let Some(json) = read_zip_file(&mut archive, "timers.json")? {
        let parsed: Vec<SeTimer> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for t in parsed {
            let id = timer_key(&t);
            let interval = se_timer_interval_mins(&t);
            let message = se_timer_message(&t);
            let enabled = t.enabled;
            let name = if t.name.is_empty() {
                format!("Timer {interval}")
            } else {
                t.name
            };
            timers.push(SeTimerPreview {
                id,
                name,
                message,
                interval,
                enabled,
            });
        }
    }

    let mut variables = Vec::new();
    if let Some(json) = read_zip_file(&mut archive, "variables.json")? {
        let parsed: Vec<SeVariable> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for v in parsed {
            let name = db::normalize_var_name(&v.name);
            if name.is_empty() {
                continue;
            }
            variables.push(SeVariablePreview {
                id: var_key(&v),
                name,
                value: v.value,
            });
        }
    }

    let automations = parse_automations_from_archive(&mut archive)?;

    if commands.is_empty() && timers.is_empty() && variables.is_empty() && automations.is_empty() {
        return Err(
            "Nothing importable found. Use an export.stream ZIP with commands/, timers/, and/or variables/."
                .into(),
        );
    }

    Ok(SeImportPreview {
        commands,
        timers,
        variables,
        automations,
    })
}

pub fn import_selected(
    conn: &Connection,
    path: &str,
    command_ids: &[String],
    timer_ids: &[String],
    variable_ids: &[String],
    automation_ids: &[String],
    on_collision: &str,
) -> Result<ImportResult, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let existing_cmds = db::list_commands(conn)?;
    let existing_vars = db::list_variables(conn)?;
    let existing_autos = db::list_automations(conn)?;

    let mut imported_commands = 0;
    let mut imported_timers = 0;
    let mut imported_variables = 0;
    let mut imported_automations = 0;
    let mut skipped = 0;

    if let Some(json) = read_zip_file(&mut archive, "custom-commands.json")? {
        let parsed: Vec<SeCommand> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for c in parsed {
            let id = cmd_key(&c);
            if !command_ids.iter().any(|sel| sel == &id) {
                continue;
            }

            let name = c.command.trim().trim_start_matches('!').to_lowercase();
            let exists = existing_cmds.iter().any(|e| e.name == name);
            if exists && on_collision == "skip" {
                skipped += 1;
                continue;
            }

            let permission = match c.access_level {
                1500 => "broadcaster",
                500..=1499 => "mod",
                400..=499 => "sub",
                300..=399 => "vip",
                _ => "everyone",
            }
            .to_string();

            let cmd = ChatCommand {
                id: if exists && on_collision == "overwrite" {
                    existing_cmds
                        .iter()
                        .find(|e| e.name == name)
                        .map(|e| e.id.clone())
                        .unwrap_or_else(|| Uuid::new_v4().to_string())
                } else {
                    Uuid::new_v4().to_string()
                },
                name,
                aliases: String::new(),
                response: normalize_se_message(&c.reply),
                enabled: c.enabled,
                permission,
                global_cooldown: if c.cooldown.global > 0 {
                    c.cooldown.global
                } else {
                    5
                },
                user_cooldown: if c.cooldown.user > 0 {
                    c.cooldown.user
                } else {
                    15
                },
                media_id: None,
            };
            db::upsert_command(conn, cmd)?;
            imported_commands += 1;
        }
    }

    drop(archive);
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    if let Some(json) = read_zip_file(&mut archive, "timers.json")? {
        let parsed: Vec<SeTimer> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for t in parsed {
            let id = timer_key(&t);
            if !timer_ids.iter().any(|sel| sel == &id) {
                continue;
            }
            let interval = se_timer_interval_mins(&t);
            let message = se_timer_message(&t);
            let live_only = se_timer_live_only(&t);
            let min_chat_lines = t.chat_lines;
            let enabled = t.enabled;
            let name = if t.name.is_empty() {
                format!("Imported {interval}")
            } else {
                t.name
            };
            let timer = ChatTimer {
                id: Uuid::new_v4().to_string(),
                name,
                message,
                interval_mins: interval,
                min_chat_lines,
                enabled,
                live_only,
            };
            db::upsert_timer(conn, timer)?;
            imported_timers += 1;
        }
    }

    drop(archive);
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    if let Some(json) = read_zip_file(&mut archive, "variables.json")? {
        let parsed: Vec<SeVariable> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for v in parsed {
            let id = var_key(&v);
            if !variable_ids.iter().any(|sel| sel == &id) {
                continue;
            }
            let name = db::normalize_var_name(&v.name);
            if name.is_empty() {
                continue;
            }
            let exists = existing_vars.iter().any(|e| e.name == name);
            if exists && on_collision == "skip" {
                skipped += 1;
                continue;
            }
            let var = CustomVariable {
                id: if exists && on_collision == "overwrite" {
                    existing_vars
                        .iter()
                        .find(|e| e.name == name)
                        .map(|e| e.id.clone())
                        .unwrap_or_else(|| Uuid::new_v4().to_string())
                } else {
                    Uuid::new_v4().to_string()
                },
                name,
                value: v.value,
            };
            db::upsert_variable(conn, var)?;
            imported_variables += 1;
        }
    }

    drop(archive);
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let autos = parse_automations_from_archive(&mut archive)?;
    for auto in autos {
        if !automation_ids.iter().any(|sel| sel == &auto.id) {
            continue;
        }
        let exists = existing_autos
            .iter()
            .find(|e| e.trigger_type == auto.trigger_type && e.action_type == "chat");
        if exists.is_some() && on_collision == "skip" {
            skipped += 1;
            continue;
        }
        let row = Automation {
            id: if let (Some(ex), true) = (exists, on_collision == "overwrite") {
                ex.id.clone()
            } else {
                Uuid::new_v4().to_string()
            },
            name: auto.name,
            trigger_type: auto.trigger_type,
            action_type: "chat".into(),
            action_payload: auto.action_payload,
            enabled: auto.enabled,
            cooldown_secs: 0,
        };
        db::upsert_automation(conn, row)?;
        imported_automations += 1;
    }

    Ok(ImportResult {
        imported_commands,
        imported_timers,
        imported_giveaways: 0,
        imported_automations,
        imported_variables,
        skipped,
    })
}
