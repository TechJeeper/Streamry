use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::db;
use crate::models::{
    ActiveGiveawayView, Giveaway, GiveawayRunHistory, GiveawayWinner, IncomingChat,
};
use crate::AppState;

pub fn get_active_view(conn: &rusqlite::Connection) -> Result<Option<ActiveGiveawayView>, String> {
    let row: Option<(String, String, String, Option<String>, String, String)> = conn
        .query_row(
            "SELECT r.id, r.giveaway_id, r.status, r.ends_at, r.started_at, r.winners_json
             FROM giveaway_runs r WHERE r.status = 'open' ORDER BY r.started_at DESC LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let Some((run_id, gw_id, status, ends_at, started_at, winners_json)) = row else {
        return Ok(None);
    };

    let giveaways = db::list_giveaways(conn)?;
    let giveaway = giveaways
        .into_iter()
        .find(|g| g.id == gw_id)
        .ok_or_else(|| "Giveaway definition missing.".to_string())?;

    let entry_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM giveaway_entries WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let winners: Vec<GiveawayWinner> =
        serde_json::from_str(&winners_json).unwrap_or_default();

    Ok(Some(ActiveGiveawayView {
        giveaway,
        run_id,
        status,
        started_at,
        ends_at,
        entry_count,
        winners,
    }))
}

pub fn start(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    // Close any open run
    conn.execute(
        "UPDATE giveaway_runs SET status = 'closed' WHERE status = 'open'",
        [],
    )
    .map_err(|e| e.to_string())?;

    let gw = db::list_giveaways(conn)?
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| "Giveaway not found.".to_string())?;

    let run_id = Uuid::new_v4().to_string();
    let started = Utc::now();
    let ends_at = gw
        .duration_mins
        .map(|m| (started + Duration::minutes(m)).to_rfc3339());

    conn.execute(
        "INSERT INTO giveaway_runs(id, giveaway_id, status, started_at, ends_at, winners_json)
         VALUES(?1,?2,'open',?3,?4,'[]')",
        params![run_id, gw.id, started.to_rfc3339(), ends_at],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn stop_active(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE giveaway_runs SET status = 'closed' WHERE status = 'open'",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Cryptographically secure uniform draw via OsRng (CSPRNG).
pub fn draw_winners(conn: &rusqlite::Connection) -> Result<Vec<GiveawayWinner>, String> {
    let active = get_active_view(conn)?.ok_or_else(|| "No open giveaway.".to_string())?;
    if !active.winners.is_empty() {
        return Ok(active.winners);
    }

    let mut entries: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT user_id, login FROM giveaway_entries WHERE run_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![active.run_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    if entries.is_empty() {
        return Err("No one has entered yet.".into());
    }

    let count = active.giveaway.winner_count.max(1) as usize;
    let count = count.min(entries.len());

    // Fisher–Yates via SliceRandom + OsRng — every entrant equal probability
    let mut rng = OsRng;
    entries.shuffle(&mut rng);
    let winners: Vec<GiveawayWinner> = entries
        .into_iter()
        .take(count)
        .map(|(user_id, login)| GiveawayWinner { user_id, login })
        .collect();

    let json = serde_json::to_string(&winners).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE giveaway_runs SET winners_json = ?1, status = 'closed' WHERE id = ?2",
        params![json, active.run_id],
    )
    .map_err(|e| e.to_string())?;

    // Trust log metadata (no RNG seed)
    let _ = db::set_setting(
        conn,
        "last_giveaway_draw",
        &serde_json::json!({
            "at": Utc::now().to_rfc3339(),
            "runId": active.run_id,
            "entryCount": active.entry_count,
            "winners": winners.iter().map(|w| &w.login).collect::<Vec<_>>(),
        })
        .to_string(),
    );

    Ok(winners)
}

pub fn format_announce(
    conn: &rusqlite::Connection,
    winners: &[GiveawayWinner],
) -> Result<String, String> {
    // Use last closed run
    let (template, prize, entries): (String, String, i64) = conn
        .query_row(
            "SELECT g.announce_template, g.prize,
                    (SELECT COUNT(*) FROM giveaway_entries e WHERE e.run_id = r.id)
             FROM giveaway_runs r
             JOIN giveaways g ON g.id = r.giveaway_id
             ORDER BY r.started_at DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    let winner_str = winners
        .iter()
        .map(|w| format!("@{}", w.login))
        .collect::<Vec<_>>()
        .join(", ");

    Ok(template
        .replace("${winner}", &winner_str)
        .replace("${prize}", &prize)
        .replace("${entries}", &entries.to_string()))
}

pub fn list_winner_history(
    conn: &rusqlite::Connection,
    limit: i64,
) -> Result<Vec<GiveawayRunHistory>, String> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.giveaway_id, g.title, g.prize, r.started_at, r.ends_at, r.winners_json,
                    (SELECT COUNT(*) FROM giveaway_entries e WHERE e.run_id = r.id)
             FROM giveaway_runs r
             JOIN giveaways g ON g.id = r.giveaway_id
             WHERE r.status = 'closed'
               AND r.winners_json IS NOT NULL
               AND r.winners_json != ''
               AND r.winners_json != '[]'
             ORDER BY r.started_at DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |r| {
            let winners_json: String = r.get(6)?;
            let winners: Vec<GiveawayWinner> =
                serde_json::from_str(&winners_json).unwrap_or_default();
            Ok(GiveawayRunHistory {
                run_id: r.get(0)?,
                giveaway_id: r.get(1)?,
                title: r.get(2)?,
                prize: r.get(3)?,
                started_at: r.get(4)?,
                ends_at: r.get(5)?,
                winners,
                entry_count: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let item = row.map_err(|e| e.to_string())?;
        if !item.winners.is_empty() {
            out.push(item);
        }
    }
    Ok(out)
}

pub fn handle_chat(app: &AppHandle, chat: &IncomingChat) -> Option<Option<String>> {
    let state = app.state::<AppState>();
    let db = state.db.lock();
    let active = get_active_view(&db).ok()??;
    let msg = chat.message.trim().to_lowercase();
    let entry = active.giveaway.entry_command.to_lowercase();
    let draw = active.giveaway.draw_command.to_lowercase();

    if msg == entry || msg.starts_with(&format!("{entry} ")) {
        // Eligibility
        if active.giveaway.eligibility == "sub" && !chat.is_sub && !chat.is_mod {
            return Some(None);
        }
        if active.giveaway.exclude_mods && chat.is_mod && !chat.is_broadcaster {
            return Some(None);
        }
        if chat.is_broadcaster {
            return Some(None);
        }
        let now = Utc::now().to_rfc3339();
        let inserted = db
            .execute(
                "INSERT OR IGNORE INTO giveaway_entries(run_id, user_id, login, entered_at)
                 VALUES(?1,?2,?3,?4)",
                params![active.run_id, chat.user_id, chat.login, now],
            )
            .ok()?;
        let _ = app.emit("giveaway-updated", ());
        if inserted > 0 && active.giveaway.confirm_entry {
            return Some(Some(format!("@{} you're in! Good luck ✨", chat.display)));
        }
        return Some(None);
    }

    if msg == draw || msg.starts_with(&format!("{draw} ")) {
        if !(chat.is_mod || chat.is_broadcaster) {
            return Some(None);
        }
        drop(db);
        match draw_winners(&state.db.lock()) {
            Ok(winners) if !winners.is_empty() => {
                let announce = format_announce(&state.db.lock(), &winners).ok()?;
                let _ = app.emit("giveaway-updated", ());
                return Some(Some(announce));
            }
            Ok(_) => return Some(None),
            Err(e) => return Some(Some(e)),
        }
    }

    None
}

#[allow(dead_code)]
pub fn _type_check(_: &Giveaway) {}
