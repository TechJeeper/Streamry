use std::fs::File;
use std::io::{Read, Write};

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::db;
use crate::models::*;

#[derive(Serialize, Deserialize)]
struct BackupFile {
    version: u32,
    exported_at: String,
    commands: Vec<ChatCommand>,
    timers: Vec<ChatTimer>,
    giveaways: Vec<Giveaway>,
    automations: Vec<Automation>,
    #[serde(default)]
    variables: Vec<CustomVariable>,
}

pub fn export_backup(conn: &Connection, path: &str) -> Result<(), String> {
    let backup = BackupFile {
        version: 2,
        exported_at: Utc::now().to_rfc3339(),
        commands: db::list_commands(conn)?,
        timers: db::list_timers(conn)?,
        giveaways: db::list_giveaways(conn)?,
        automations: db::list_automations(conn)?,
        variables: db::list_variables(conn)?,
    };
    let json = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;

    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    zip.start_file("backup.json", SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    zip.write_all(json.as_bytes())
        .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn read_backup(path: &str) -> Result<BackupFile, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut found = None;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        if f.name().ends_with("backup.json") {
            let mut s = String::new();
            f.read_to_string(&mut s).map_err(|e| e.to_string())?;
            found = Some(s);
            break;
        }
    }
    let json = found.ok_or_else(|| "Invalid backup: missing backup.json".to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn preview_backup(path: &str) -> Result<BackupPreview, String> {
    let b = read_backup(path)?;
    Ok(BackupPreview {
        commands: b.commands.len(),
        timers: b.timers.len(),
        giveaways: b.giveaways.len(),
        automations: b.automations.len(),
        variables: b.variables.len(),
        exported_at: b.exported_at,
    })
}

pub fn restore_backup(
    conn: &Connection,
    path: &str,
    include_commands: bool,
    include_timers: bool,
    include_giveaways: bool,
    include_automations: bool,
    include_variables: bool,
    replace: bool,
) -> Result<ImportResult, String> {
    let b = read_backup(path)?;
    let mut result = ImportResult {
        imported_commands: 0,
        imported_timers: 0,
        imported_giveaways: 0,
        imported_automations: 0,
        imported_variables: 0,
        skipped: 0,
    };

    if include_commands {
        if replace {
            for c in db::list_commands(conn)? {
                db::delete_command(conn, &c.id)?;
            }
        }
        for mut c in b.commands {
            if !replace {
                c.id = Uuid::new_v4().to_string();
            }
            db::upsert_command(conn, c)?;
            result.imported_commands += 1;
        }
    }
    if include_timers {
        if replace {
            for t in db::list_timers(conn)? {
                db::delete_timer(conn, &t.id)?;
            }
        }
        for mut t in b.timers {
            if !replace {
                t.id = Uuid::new_v4().to_string();
            }
            db::upsert_timer(conn, t)?;
            result.imported_timers += 1;
        }
    }
    if include_giveaways {
        if replace {
            for g in db::list_giveaways(conn)? {
                db::delete_giveaway(conn, &g.id)?;
            }
        }
        for mut g in b.giveaways {
            if !replace {
                g.id = Uuid::new_v4().to_string();
            }
            db::upsert_giveaway(conn, g)?;
            result.imported_giveaways += 1;
        }
    }
    if include_automations {
        if replace {
            for a in db::list_automations(conn)? {
                db::delete_automation(conn, &a.id)?;
            }
        }
        for mut a in b.automations {
            if !replace {
                a.id = Uuid::new_v4().to_string();
            }
            db::upsert_automation(conn, a)?;
            result.imported_automations += 1;
        }
    }
    if include_variables {
        if replace {
            for v in db::list_variables(conn)? {
                db::delete_variable(conn, &v.id)?;
            }
        }
        for mut v in b.variables {
            if !replace {
                v.id = Uuid::new_v4().to_string();
            }
            db::upsert_variable(conn, v)?;
            result.imported_variables += 1;
        }
    }

    Ok(result)
}
