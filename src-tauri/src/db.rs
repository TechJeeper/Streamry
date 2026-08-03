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
            volume INTEGER NOT NULL DEFAULT 80,
            overlay_position TEXT NOT NULL DEFAULT 'center',
            always_show INTEGER NOT NULL DEFAULT 0,
            overlay_x INTEGER NOT NULL DEFAULT 4,
            overlay_y INTEGER NOT NULL DEFAULT 2,
            overlay_w INTEGER NOT NULL DEFAULT 8,
            overlay_h INTEGER NOT NULL DEFAULT 5,
            chroma_key TEXT NOT NULL DEFAULT '',
            chroma_tolerance INTEGER NOT NULL DEFAULT 48
        );
        "#,
    )
    .map_err(|e| e.to_string())?;

    // Additive migrations for existing installs
    let _ = conn.execute("ALTER TABLE commands ADD COLUMN media_id TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN overlay_position TEXT NOT NULL DEFAULT 'center'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN always_show INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN overlay_x INTEGER NOT NULL DEFAULT 4",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN overlay_y INTEGER NOT NULL DEFAULT 2",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN overlay_w INTEGER NOT NULL DEFAULT 8",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN overlay_h INTEGER NOT NULL DEFAULT 5",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN chroma_key TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE media_clips ADD COLUMN chroma_tolerance INTEGER NOT NULL DEFAULT 48",
        [],
    );
    // One-time migrate named anchors → 16×9 grid rects (only when still on defaults)
    let _ = conn.execute_batch(
        r#"
        UPDATE media_clips SET overlay_x=0,  overlay_y=0, overlay_w=5, overlay_h=3 WHERE overlay_position='top-left'    AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=5,  overlay_y=0, overlay_w=6, overlay_h=3 WHERE overlay_position='top'         AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=11, overlay_y=0, overlay_w=5, overlay_h=3 WHERE overlay_position='top-right'   AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=0,  overlay_y=3, overlay_w=5, overlay_h=3 WHERE overlay_position='left'        AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=11, overlay_y=3, overlay_w=5, overlay_h=3 WHERE overlay_position='right'       AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=0,  overlay_y=6, overlay_w=5, overlay_h=3 WHERE overlay_position='bottom-left' AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=5,  overlay_y=6, overlay_w=6, overlay_h=3 WHERE overlay_position='bottom'      AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        UPDATE media_clips SET overlay_x=11, overlay_y=6, overlay_w=5, overlay_h=3 WHERE overlay_position='bottom-right'AND overlay_x=4 AND overlay_y=2 AND overlay_w=8 AND overlay_h=5;
        "#,
    );

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

const MEDIA_SELECT: &str = "SELECT id, name, media_type, file_name, duration_ms, volume,
        COALESCE(always_show, 0),
        COALESCE(overlay_x, 4), COALESCE(overlay_y, 2),
        COALESCE(overlay_w, 8), COALESCE(overlay_h, 5),
        COALESCE(chroma_key, ''), COALESCE(chroma_tolerance, 48)
     FROM media_clips";

fn row_to_media(r: &rusqlite::Row<'_>) -> rusqlite::Result<MediaClip> {
    Ok(MediaClip {
        id: r.get(0)?,
        name: r.get(1)?,
        media_type: r.get(2)?,
        file_name: r.get(3)?,
        duration_ms: r.get(4)?,
        volume: r.get(5)?,
        always_show: r.get::<_, i64>(6)? != 0,
        overlay_x: r.get(7)?,
        overlay_y: r.get(8)?,
        overlay_w: r.get(9)?,
        overlay_h: r.get(10)?,
        chroma_key: r.get(11)?,
        chroma_tolerance: r.get(12)?,
    })
}

fn clamp_overlay_rect(clip: &mut MediaClip) {
    use crate::models::{OVERLAY_GRID_H, OVERLAY_GRID_W};
    clip.overlay_w = clip.overlay_w.clamp(1, OVERLAY_GRID_W);
    clip.overlay_h = clip.overlay_h.clamp(1, OVERLAY_GRID_H);
    clip.overlay_x = clip.overlay_x.clamp(0, OVERLAY_GRID_W - clip.overlay_w);
    clip.overlay_y = clip.overlay_y.clamp(0, OVERLAY_GRID_H - clip.overlay_h);
}

fn normalize_chroma_key(s: &str) -> String {
    let t = s.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return format!("#{}", t.to_ascii_uppercase());
    }
    if t.len() == 3 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let b = t.as_bytes();
        return format!(
            "#{0}{0}{1}{1}{2}{2}",
            b[0] as char, b[1] as char, b[2] as char
        )
        .to_ascii_uppercase();
    }
    String::new()
}

pub fn list_media(conn: &Connection) -> Result<Vec<MediaClip>, String> {
    let mut stmt = conn
        .prepare(&format!("{MEDIA_SELECT} ORDER BY name COLLATE NOCASE"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_media)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn get_media(conn: &Connection, id: &str) -> Result<Option<MediaClip>, String> {
    conn.query_row(
        &format!("{MEDIA_SELECT} WHERE id = ?1"),
        params![id],
        row_to_media,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get_media_by_name(conn: &Connection, name: &str) -> Result<Option<MediaClip>, String> {
    let key = name.trim().to_lowercase();
    let mut stmt = conn
        .prepare(MEDIA_SELECT)
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_media)
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
    clamp_overlay_rect(&mut clip);
    if clip.media_type != "image" {
        clip.always_show = false;
    }
    if matches!(clip.media_type.as_str(), "image" | "gif" | "video") {
        clip.chroma_key = normalize_chroma_key(&clip.chroma_key);
        clip.chroma_tolerance = clip.chroma_tolerance.clamp(0, 120);
    } else {
        clip.chroma_key.clear();
        clip.chroma_tolerance = crate::models::default_chroma_tolerance();
    }
    conn.execute(
        "INSERT INTO media_clips(id, name, media_type, file_name, duration_ms, volume,
            always_show, overlay_x, overlay_y, overlay_w, overlay_h, chroma_key, chroma_tolerance)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, media_type=excluded.media_type, file_name=excluded.file_name,
           duration_ms=excluded.duration_ms, volume=excluded.volume,
           always_show=excluded.always_show,
           overlay_x=excluded.overlay_x, overlay_y=excluded.overlay_y,
           overlay_w=excluded.overlay_w, overlay_h=excluded.overlay_h,
           chroma_key=excluded.chroma_key, chroma_tolerance=excluded.chroma_tolerance",
        params![
            clip.id,
            clip.name,
            clip.media_type,
            clip.file_name,
            clip.duration_ms,
            clip.volume,
            if clip.always_show { 1 } else { 0 },
            clip.overlay_x,
            clip.overlay_y,
            clip.overlay_w,
            clip.overlay_h,
            clip.chroma_key,
            clip.chroma_tolerance,
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
