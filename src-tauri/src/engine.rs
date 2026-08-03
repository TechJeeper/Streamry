use std::collections::HashMap;

use chrono::{Duration, Utc};
use rusqlite::params;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::giveaway;
use crate::media as media_mod;
use crate::models::{IncomingChat, Automation};
use crate::{AppState, ChatOutbound};

pub fn handle_message(
    app: &AppHandle,
    chat: &IncomingChat,
    channel: &str,
    global_cd: &mut HashMap<String, i64>,
    user_cd: &mut HashMap<String, i64>,
) -> Option<String> {
    let msg = chat.message.trim();
    if msg.is_empty() {
        return None;
    }

    // Giveaway entry / draw first
    if let Some(reply) = giveaway::handle_chat(app, chat) {
        return reply;
    }

    if !msg.starts_with('!') {
        return None;
    }

    let trigger = msg
        .split_whitespace()
        .next()?
        .trim_start_matches('!')
        .to_lowercase();

    let state = app.state::<AppState>();
    let commands = db::list_commands(&state.db.lock()).ok()?;
    let cmd = commands.into_iter().find(|c| {
        if !c.enabled {
            return false;
        }
        if c.name == trigger {
            return true;
        }
        c.aliases
            .split(',')
            .map(|a| a.trim().trim_start_matches('!').to_lowercase())
            .any(|a| a == trigger)
    })?;

    if !permission_ok(&cmd.permission, chat) {
        return None;
    }

    let now = Utc::now().timestamp();
    if let Some(until) = global_cd.get(&cmd.id) {
        if now < *until {
            return None;
        }
    }
    let ukey = format!("{}:{}", cmd.id, chat.user_id);
    if let Some(until) = user_cd.get(&ukey) {
        if now < *until {
            return None;
        }
    }
    global_cd.insert(cmd.id.clone(), now + cmd.global_cooldown);
    user_cd.insert(ukey, now + cmd.user_cooldown);

    let custom_vars = {
        let state = app.state::<AppState>();
        let vars = db::list_variables(&state.db.lock()).unwrap_or_default();
        vars
    };

    // Optional OBS media clip
    if let Some(media_id) = cmd.media_id.as_ref().filter(|s| !s.is_empty()) {
        play_media_by_ref(app, media_id);
    }

    let response = cmd.response.trim();
    if response.is_empty() {
        return None;
    }

    Some(render_vars(
        response,
        chat,
        channel,
        None,
        &custom_vars,
    ))
}

fn permission_ok(perm: &str, chat: &IncomingChat) -> bool {
    match perm {
        "broadcaster" => chat.is_broadcaster,
        "mod" => chat.is_mod || chat.is_broadcaster,
        "vip" => chat.is_vip || chat.is_mod || chat.is_broadcaster,
        "sub" => chat.is_sub || chat.is_vip || chat.is_mod || chat.is_broadcaster,
        _ => true,
    }
}

pub fn render_vars(
    template: &str,
    chat: &IncomingChat,
    channel: &str,
    extra: Option<&[(&str, &str)]>,
    custom_vars: &[crate::models::CustomVariable],
) -> String {
    let mut out = template
        .replace("${user}", &chat.display)
        .replace("${login}", &chat.login)
        .replace("${channel}", channel)
        .replace("${uptime}", "live");
    if let Some(pairs) = extra {
        for (k, v) in pairs {
            out = out.replace(k, v);
        }
    }
    for v in custom_vars {
        out = out.replace(&format!("${{{}}}", v.name), &v.value);
    }
    out
}

pub fn handle_usernotice(app: &AppHandle, event: &crate::chat::UserNoticeEvent) {
    let state = app.state::<AppState>();
    let autos = match db::list_automations(&state.db.lock()) {
        Ok(a) => a,
        Err(_) => return,
    };
    for auto in autos.into_iter().filter(|a| a.enabled) {
        if auto.trigger_type == event.kind || auto.trigger_type == "subscribe" && event.kind == "subscribe" {
            run_automation(app, &auto, &event.display);
        }
    }
}

fn run_automation(app: &AppHandle, auto: &Automation, user: &str) {
    match auto.action_type.as_str() {
        "chat" => {
            let state = app.state::<AppState>();
            let channel = state
                .runtime
                .lock()
                .channel
                .clone()
                .unwrap_or_default();
            let custom_vars = db::list_variables(&state.db.lock()).unwrap_or_default();
            let dummy = IncomingChat {
                user_id: String::new(),
                login: user.to_lowercase(),
                display: user.to_string(),
                message: String::new(),
                is_mod: false,
                is_vip: false,
                is_sub: false,
                is_broadcaster: false,
            };
            let msg = render_vars(&auto.action_payload, &dummy, &channel, None, &custom_vars);
            let tx = state.chat_tx.lock().clone();
            if let Some(tx) = tx {
                let _ = tx.send(ChatOutbound::Message(msg));
            }
        }
        "enable_command" | "disable_command" => {
            let enable = auto.action_type.starts_with("enable");
            let state = app.state::<AppState>();
            let db = state.db.lock();
            if let Ok(mut cmds) = db::list_commands(&db) {
                if let Some(cmd) = cmds.iter_mut().find(|c| c.name == auto.action_payload || c.id == auto.action_payload) {
                    cmd.enabled = enable;
                    let _ = db::upsert_command(&db, cmd.clone());
                }
            }
        }
        "enable_timer" | "disable_timer" => {
            let enable = auto.action_type.starts_with("enable");
            let state = app.state::<AppState>();
            let db = state.db.lock();
            if let Ok(mut timers) = db::list_timers(&db) {
                if let Some(t) = timers.iter_mut().find(|t| t.name == auto.action_payload || t.id == auto.action_payload) {
                    t.enabled = enable;
                    let _ = db::upsert_timer(&db, t.clone());
                }
            }
        }
        "play_media" => {
            play_media_by_ref(app, &auto.action_payload);
        }
        _ => {}
    }
}

fn play_media_by_ref(app: &AppHandle, key: &str) {
    let state = app.state::<AppState>();
    let clip = {
        let db = state.db.lock();
        db::get_media(&db, key)
            .ok()
            .flatten()
            .or_else(|| db::get_media_by_name(&db, key).ok().flatten())
    };
    if let Some(clip) = clip {
        media_mod::play_clip(&state.overlay, &clip);
    }
}

pub async fn run_scheduler(app: AppHandle) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        // Auto-draw giveaways past ends_at
        {
            let state = app.state::<AppState>();
            let active = {
                let db = state.db.lock();
                giveaway::get_active_view(&db).ok().flatten()
            };
            if let Some(active) = active {
                if let Some(ends) = &active.ends_at {
                    if let Ok(ends_at) = chrono::DateTime::parse_from_rfc3339(ends) {
                        if Utc::now() >= ends_at.with_timezone(&Utc) && active.winners.is_empty() {
                            let draw_result = {
                                let db = state.db.lock();
                                giveaway::draw_winners(&db)
                            };
                            if let Ok(winners) = draw_result {
                                if !winners.is_empty() {
                                    let announce = {
                                        let db = state.db.lock();
                                        giveaway::format_announce(&db, &winners).ok()
                                    };
                                    if let Some(announce) = announce {
                                        let tx = state.chat_tx.lock().clone();
                                        if let Some(tx) = tx {
                                            let _ = tx.send(ChatOutbound::Message(announce));
                                        }
                                    }
                                }
                                let _ = app.emit("giveaway-updated", ());
                            }
                        }
                    }
                }
            }
        }

        // Timers
        let (timers, chat_lines, live, live_only_global) = {
            let state = app.state::<AppState>();
            let db = state.db.lock();
            let timers = db::list_timers(&db).unwrap_or_default();
            let rt = state.runtime.lock();
            let live_only = db::get_setting(&db, "timers_live_only")
                .ok()
                .flatten()
                .as_deref()
                == Some("1");
            (timers, rt.chat_lines, rt.live, live_only)
        };

        for timer in timers.into_iter().filter(|t| t.enabled) {
            if (timer.live_only || live_only_global) && !live {
                continue;
            }
            let state = app.state::<AppState>();
            let db = state.db.lock();
            let last: Option<String> = db
                .query_row(
                    "SELECT last_fired_at FROM timer_state WHERE timer_id = ?1",
                    params![timer.id],
                    |r| r.get(0),
                )
                .ok();
            let due = match last {
                None => true,
                Some(ts) => chrono::DateTime::parse_from_rfc3339(&ts)
                    .map(|t| Utc::now() >= t.with_timezone(&Utc) + Duration::minutes(timer.interval_mins))
                    .unwrap_or(true),
            };
            if !due {
                continue;
            }
            let lines_at: i64 = db
                .query_row(
                    "SELECT lines_at_fire FROM timer_state WHERE timer_id = ?1",
                    params![timer.id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if timer.min_chat_lines > 0 && (chat_lines as i64) - lines_at < timer.min_chat_lines {
                continue;
            }
            let channel = state
                .runtime
                .lock()
                .channel
                .clone()
                .unwrap_or_default();
            let custom_vars = db::list_variables(&db).unwrap_or_default();
            let tx = state.chat_tx.lock().clone();
            if let Some(tx) = tx {
                let dummy = IncomingChat {
                    user_id: String::new(),
                    login: String::new(),
                    display: String::new(),
                    message: String::new(),
                    is_mod: false,
                    is_vip: false,
                    is_sub: false,
                    is_broadcaster: false,
                };
                let msg = render_vars(&timer.message, &dummy, &channel, None, &custom_vars);
                let _ = tx.send(ChatOutbound::Message(msg));
            }
            let now = Utc::now().to_rfc3339();
            let _ = db.execute(
                "INSERT INTO timer_state(timer_id, last_fired_at, lines_at_fire) VALUES(?1,?2,?3)
                 ON CONFLICT(timer_id) DO UPDATE SET last_fired_at=excluded.last_fired_at, lines_at_fire=excluded.lines_at_fire",
                params![timer.id, now, chat_lines as i64],
            );
        }

        // Lightweight live check via Helix when connected
        check_live_status(&app).await;
    }
}

async fn check_live_status(app: &AppHandle) {
    let (client_id, channel, connected) = {
        let state = app.state::<AppState>();
        let db = state.db.lock();
        let rt = state.runtime.lock();
        (
            db::get_setting(&db, "client_id").ok().flatten().unwrap_or_default(),
            db::get_setting(&db, "channel").ok().flatten().unwrap_or_default(),
            rt.connected,
        )
    };
    if !connected || client_id.is_empty() || channel.is_empty() {
        return;
    }
    let token = match crate::auth::load_access_token(&client_id).await {
        Ok(t) => t,
        Err(_) => return,
    };
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.twitch.tv/helix/streams?user_login={}",
        urlencoding::encode(&channel)
    );
    if let Ok(resp) = client
        .get(&url)
        .header("Client-Id", &client_id)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let live = json
                .get("data")
                .and_then(|d| d.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let state = app.state::<AppState>();
            let mut rt = state.runtime.lock();
            if rt.live != live {
                rt.live = live;
                let _ = app.emit("status-changed", rt.clone());
                // stream online/offline automations
                let trigger = if live { "stream_online" } else { "stream_offline" };
                drop(rt);
                if let Ok(autos) = db::list_automations(&state.db.lock()) {
                    for auto in autos.into_iter().filter(|a| a.enabled && a.trigger_type == trigger) {
                        run_automation(app, &auto, &channel);
                    }
                }
            }
        }
    }
}
