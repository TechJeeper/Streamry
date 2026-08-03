use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::models::*;

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS commands (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            aliases TEXT NOT NULL DEFAULT '',
            response TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            permission TEXT NOT NULL DEFAULT 'everyone',
            global_cooldown INTEGER NOT NULL DEFAULT 5,
            user_cooldown INTEGER NOT NULL DEFAULT 15
        );
        CREATE TABLE IF NOT EXISTS timers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            message TEXT NOT NULL,
            interval_mins INTEGER NOT NULL DEFAULT 10,
            min_chat_lines INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            live_only INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS giveaways (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            prize TEXT NOT NULL DEFAULT '',
            entry_command TEXT NOT NULL,
            draw_command TEXT NOT NULL DEFAULT '!pickwinner',
            duration_mins INTEGER,
            winner_count INTEGER NOT NULL DEFAULT 1,
            eligibility TEXT NOT NULL DEFAULT 'everyone',
            exclude_mods INTEGER NOT NULL DEFAULT 0,
            confirm_entry INTEGER NOT NULL DEFAULT 0,
            announce_template TEXT NOT NULL DEFAULT '🎉 Congratulations ${winner}! You won ${prize}! (${entries} entered)',
            enabled INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS giveaway_runs (
            id TEXT PRIMARY KEY,
            giveaway_id TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ends_at TEXT,
            winners_json TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY(giveaway_id) REFERENCES giveaways(id)
        );
        CREATE TABLE IF NOT EXISTS giveaway_entries (
            run_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            login TEXT NOT NULL,
            entered_at TEXT NOT NULL,
            PRIMARY KEY (run_id, user_id)
        );
        CREATE TABLE IF NOT EXISTS automations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            trigger_type TEXT NOT NULL,
            action_type TEXT NOT NULL,
            action_payload TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            cooldown_secs INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS timer_state (
            timer_id TEXT PRIMARY KEY,
            last_fired_at TEXT,
            lines_at_fire INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS variables (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            value TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS media_clips (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            media_type TEXT NOT NULL,
            file_name TEXT NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 5000,
            volume INTEGER NOT NULL DEFAULT 80
        );
        "#,
    )
    .map_err(|e| e.to_string())?;

    // Additive migrations for existing installs
    let _ = conn.execute("ALTER TABLE commands ADD COLUMN media_id TEXT", []);

    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_commands(conn: &Connection) -> Result<Vec<ChatCommand>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, aliases, response, enabled, permission, global_cooldown, user_cooldown, media_id
             FROM commands ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let media_id: Option<String> = r.get(8)?;
            Ok(ChatCommand {
                id: r.get(0)?,
                name: r.get(1)?,
                aliases: r.get(2)?,
                response: r.get(3)?,
                enabled: r.get::<_, i64>(4)? == 1,
                permission: r.get(5)?,
                global_cooldown: r.get(6)?,
                user_cooldown: r.get(7)?,
                media_id: media_id.filter(|s| !s.is_empty()),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn upsert_command(conn: &Connection, mut cmd: ChatCommand) -> Result<ChatCommand, String> {
    if cmd.id.is_empty() {
        cmd.id = Uuid::new_v4().to_string();
    }
    cmd.name = cmd.name.trim().trim_start_matches('!').to_lowercase();
    if let Some(mid) = &cmd.media_id {
        if mid.trim().is_empty() {
            cmd.media_id = None;
        }
    }
    conn.execute(
        "INSERT INTO commands(id, name, aliases, response, enabled, permission, global_cooldown, user_cooldown, media_id)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, aliases=excluded.aliases, response=excluded.response,
           enabled=excluded.enabled, permission=excluded.permission,
           global_cooldown=excluded.global_cooldown, user_cooldown=excluded.user_cooldown,
           media_id=excluded.media_id",
        params![
            cmd.id,
            cmd.name,
            cmd.aliases,
            cmd.response,
            if cmd.enabled { 1 } else { 0 },
            cmd.permission,
            cmd.global_cooldown,
            cmd.user_cooldown,
            cmd.media_id.clone().unwrap_or_default(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(cmd)
}

pub fn delete_command(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM commands WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_timers(conn: &Connection) -> Result<Vec<ChatTimer>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, message, interval_mins, min_chat_lines, enabled, live_only
             FROM timers ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ChatTimer {
                id: r.get(0)?,
                name: r.get(1)?,
                message: r.get(2)?,
                interval_mins: r.get(3)?,
                min_chat_lines: r.get(4)?,
                enabled: r.get::<_, i64>(5)? == 1,
                live_only: r.get::<_, i64>(6)? == 1,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn upsert_timer(conn: &Connection, mut timer: ChatTimer) -> Result<ChatTimer, String> {
    if timer.id.is_empty() {
        timer.id = Uuid::new_v4().to_string();
    }
    conn.execute(
        "INSERT INTO timers(id, name, message, interval_mins, min_chat_lines, enabled, live_only)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, message=excluded.message, interval_mins=excluded.interval_mins,
           min_chat_lines=excluded.min_chat_lines, enabled=excluded.enabled, live_only=excluded.live_only",
        params![
            timer.id,
            timer.name,
            timer.message,
            timer.interval_mins,
            timer.min_chat_lines,
            if timer.enabled { 1 } else { 0 },
            if timer.live_only { 1 } else { 0 },
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(timer)
}

pub fn delete_timer(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM timers WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_giveaways(conn: &Connection) -> Result<Vec<Giveaway>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, prize, entry_command, draw_command, duration_mins, winner_count,
                    eligibility, exclude_mods, confirm_entry, announce_template, enabled
             FROM giveaways ORDER BY title COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Giveaway {
                id: r.get(0)?,
                title: r.get(1)?,
                prize: r.get(2)?,
                entry_command: r.get(3)?,
                draw_command: r.get(4)?,
                duration_mins: r.get(5)?,
                winner_count: r.get(6)?,
                eligibility: r.get(7)?,
                exclude_mods: r.get::<_, i64>(8)? == 1,
                confirm_entry: r.get::<_, i64>(9)? == 1,
                announce_template: r.get(10)?,
                enabled: r.get::<_, i64>(11)? == 1,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn upsert_giveaway(conn: &Connection, mut gw: Giveaway) -> Result<Giveaway, String> {
    if gw.id.is_empty() {
        gw.id = Uuid::new_v4().to_string();
    }
    gw.entry_command = normalize_cmd(&gw.entry_command);
    gw.draw_command = normalize_cmd(&gw.draw_command);
    conn.execute(
        "INSERT INTO giveaways(id, title, prize, entry_command, draw_command, duration_mins,
            winner_count, eligibility, exclude_mods, confirm_entry, announce_template, enabled)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(id) DO UPDATE SET
           title=excluded.title, prize=excluded.prize, entry_command=excluded.entry_command,
           draw_command=excluded.draw_command, duration_mins=excluded.duration_mins,
           winner_count=excluded.winner_count, eligibility=excluded.eligibility,
           exclude_mods=excluded.exclude_mods, confirm_entry=excluded.confirm_entry,
           announce_template=excluded.announce_template, enabled=excluded.enabled",
        params![
            gw.id,
            gw.title,
            gw.prize,
            gw.entry_command,
            gw.draw_command,
            gw.duration_mins,
            gw.winner_count,
            gw.eligibility,
            if gw.exclude_mods { 1 } else { 0 },
            if gw.confirm_entry { 1 } else { 0 },
            gw.announce_template,
            if gw.enabled { 1 } else { 0 },
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(gw)
}

pub fn delete_giveaway(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM giveaway_entries WHERE run_id IN (SELECT id FROM giveaway_runs WHERE giveaway_id = ?1)", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM giveaway_runs WHERE giveaway_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM giveaways WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_automations(conn: &Connection) -> Result<Vec<Automation>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, trigger_type, action_type, action_payload, enabled, cooldown_secs
             FROM automations ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Automation {
                id: r.get(0)?,
                name: r.get(1)?,
                trigger_type: r.get(2)?,
                action_type: r.get(3)?,
                action_payload: r.get(4)?,
                enabled: r.get::<_, i64>(5)? == 1,
                cooldown_secs: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn upsert_automation(conn: &Connection, mut auto: Automation) -> Result<Automation, String> {
    if auto.id.is_empty() {
        auto.id = Uuid::new_v4().to_string();
    }
    conn.execute(
        "INSERT INTO automations(id, name, trigger_type, action_type, action_payload, enabled, cooldown_secs)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, trigger_type=excluded.trigger_type, action_type=excluded.action_type,
           action_payload=excluded.action_payload, enabled=excluded.enabled, cooldown_secs=excluded.cooldown_secs",
        params![
            auto.id,
            auto.name,
            auto.trigger_type,
            auto.action_type,
            auto.action_payload,
            if auto.enabled { 1 } else { 0 },
            auto.cooldown_secs,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(auto)
}

pub fn delete_automation(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM automations WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_media(conn: &Connection) -> Result<Vec<MediaClip>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, media_type, file_name, duration_ms, volume
             FROM media_clips ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(MediaClip {
                id: r.get(0)?,
                name: r.get(1)?,
                media_type: r.get(2)?,
                file_name: r.get(3)?,
                duration_ms: r.get(4)?,
                volume: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn get_media(conn: &Connection, id: &str) -> Result<Option<MediaClip>, String> {
    conn.query_row(
        "SELECT id, name, media_type, file_name, duration_ms, volume FROM media_clips WHERE id = ?1",
        params![id],
        |r| {
            Ok(MediaClip {
                id: r.get(0)?,
                name: r.get(1)?,
                media_type: r.get(2)?,
                file_name: r.get(3)?,
                duration_ms: r.get(4)?,
                volume: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get_media_by_name(conn: &Connection, name: &str) -> Result<Option<MediaClip>, String> {
    let key = name.trim().to_lowercase();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, media_type, file_name, duration_ms, volume FROM media_clips",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(MediaClip {
                id: r.get(0)?,
                name: r.get(1)?,
                media_type: r.get(2)?,
                file_name: r.get(3)?,
                duration_ms: r.get(4)?,
                volume: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let clip = row.map_err(|e| e.to_string())?;
        if clip.name.eq_ignore_ascii_case(&key) || clip.id == key {
            return Ok(Some(clip));
        }
    }
    Ok(None)
}

pub fn upsert_media(conn: &Connection, mut clip: MediaClip) -> Result<MediaClip, String> {
    if clip.id.is_empty() {
        clip.id = Uuid::new_v4().to_string();
    }
    clip.name = clip.name.trim().to_string();
    if clip.name.is_empty() {
        return Err("Media name is required".into());
    }
    clip.volume = clip.volume.clamp(0, 100);
    clip.duration_ms = clip.duration_ms.max(500);
    conn.execute(
        "INSERT INTO media_clips(id, name, media_type, file_name, duration_ms, volume)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, media_type=excluded.media_type, file_name=excluded.file_name,
           duration_ms=excluded.duration_ms, volume=excluded.volume",
        params![
            clip.id,
            clip.name,
            clip.media_type,
            clip.file_name,
            clip.duration_ms,
            clip.volume,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(clip)
}

pub fn delete_media(conn: &Connection, id: &str) -> Result<Option<MediaClip>, String> {
    let existing = get_media(conn, id)?;
    conn.execute("DELETE FROM media_clips WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    // Clear command references
    let _ = conn.execute(
        "UPDATE commands SET media_id = '' WHERE media_id = ?1",
        params![id],
    );
    Ok(existing)
}

pub fn list_variables(conn: &Connection) -> Result<Vec<CustomVariable>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, value FROM variables ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CustomVariable {
                id: r.get(0)?,
                name: r.get(1)?,
                value: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn upsert_variable(conn: &Connection, mut var: CustomVariable) -> Result<CustomVariable, String> {
    if var.id.is_empty() {
        var.id = Uuid::new_v4().to_string();
    }
    var.name = normalize_var_name(&var.name);
    if var.name.is_empty() {
        return Err("Variable name is required".into());
    }
    conn.execute(
        "INSERT INTO variables(id, name, value) VALUES(?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, value=excluded.value",
        params![var.id, var.name, var.value],
    )
    .map_err(|e| e.to_string())?;
    Ok(var)
}

pub fn delete_variable(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM variables WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Wipe bot data and settings (login tokens are cleared separately).
pub fn factory_reset(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        DELETE FROM giveaway_entries;
        DELETE FROM giveaway_runs;
        DELETE FROM giveaways;
        DELETE FROM timer_state;
        DELETE FROM timers;
        DELETE FROM commands;
        DELETE FROM automations;
        DELETE FROM variables;
        DELETE FROM media_clips;
        DELETE FROM settings;
        "#,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn normalize_var_name(s: &str) -> String {
    s.trim()
        .trim_start_matches("${")
        .trim_end_matches('}')
        .trim_start_matches('$')
        .trim()
        .to_lowercase()
}

fn normalize_cmd(s: &str) -> String {
    let t = s.trim().to_lowercase();
    if t.starts_with('!') {
        t
    } else if t.is_empty() {
        t
    } else {
        format!("!{t}")
    }
}
