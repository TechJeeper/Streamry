use std::collections::HashMap;

use native_tls::TlsConnector;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_native_tls::TlsConnector as TokioTlsConnector;

use crate::engine;
use crate::models::IncomingChat;
use crate::{AppState, ChatOutbound};

pub async fn run_chat_loop(
    app: AppHandle,
    _client_id: String,
    token: String,
    bot_login: String,
    channel: String,
    mut outbound: mpsc::UnboundedReceiver<ChatOutbound>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let tcp = TcpStream::connect("irc.chat.twitch.tv:6697")
        .await
        .map_err(|e| format!("IRC connect failed: {e}"))?;
    let tls = TokioTlsConnector::from(
        TlsConnector::new().map_err(|e| format!("TLS init failed: {e}"))?,
    );
    let stream = tls
        .connect("irc.chat.twitch.tv", tcp)
        .await
        .map_err(|e| format!("TLS handshake failed: {e}"))?;

    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    let nick = bot_login.to_lowercase();
    let chan = format!("#{}", channel.to_lowercase());

    write_line(&mut writer, "CAP REQ :twitch.tv/tags twitch.tv/commands").await?;
    write_line(&mut writer, &format!("PASS oauth:{token}")).await?;
    write_line(&mut writer, &format!("NICK {nick}")).await?;
    write_line(&mut writer, &format!("JOIN {chan}")).await?;

    let mut cooldowns: HashMap<String, i64> = HashMap::new();
    let mut user_cooldowns: HashMap<String, i64> = HashMap::new();

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    break;
                }
            }
            Some(msg) = outbound.recv() => {
                match msg {
                    ChatOutbound::Message(text) => {
                        write_line(&mut writer, &format!("PRIVMSG {chan} :{text}")).await?;
                    }
                }
            }
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(l)) => l,
                    Ok(None) => break,
                    Err(e) => return Err(e.to_string()),
                };
                if line.starts_with("PING") {
                    let pong = line.replacen("PING", "PONG", 1);
                    write_line(&mut writer, &pong).await?;
                    continue;
                }
                if let Some(chat) = parse_privmsg(&line) {
                    {
                        let state = app.state::<AppState>();
                        state.runtime.lock().chat_lines += 1;
                    }
                    let _ = app.emit("chat-message", serde_json::json!({
                        "user": chat.display,
                        "message": chat.message,
                    }));

                    // Bits arrive on PRIVMSG (bits= tag), not USERNOTICE.
                    if chat.bits > 0 {
                        engine::fire_automations(&app, "cheer", &chat.display);
                    }

                    if let Some(reply) = engine::handle_message(
                        &app,
                        &chat,
                        &channel,
                        &mut cooldowns,
                        &mut user_cooldowns,
                    ) {
                        write_line(&mut writer, &format!("PRIVMSG {chan} :{reply}")).await?;
                    }
                } else if let Some(event) = parse_usernotice(&line) {
                    engine::handle_usernotice(&app, &event);
                }
            }
        }
    }
    Ok(())
}

async fn write_line<W: AsyncWriteExt + Unpin>(w: &mut W, line: &str) -> Result<(), String> {
    w.write_all(format!("{line}\r\n").as_bytes())
        .await
        .map_err(|e| e.to_string())
}

fn parse_tags(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in raw.trim_start_matches('@').split(';') {
        if let Some((k, v)) = part.split_once('=') {
            map.insert(k.to_string(), v.replace("\\s", " "));
        }
    }
    map
}

fn parse_privmsg(line: &str) -> Option<IncomingChat> {
    let (tags, rest) = if line.starts_with('@') {
        let (t, r) = line.split_once(' ')?;
        (parse_tags(t), r)
    } else {
        (HashMap::new(), line)
    };
    if !rest.contains(" PRIVMSG ") {
        return None;
    }
    let msg = rest.split_once(" :").map(|(_, m)| m.to_string())?;
    let login = tags
        .get("display-name")
        .cloned()
        .or_else(|| {
            rest.split('!')
                .next()
                .map(|s| s.trim_start_matches(':').to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let badges = tags.get("badges").cloned().unwrap_or_default();
    let is_broadcaster = badges.contains("broadcaster/");
    let is_mod = tags.get("mod").map(|v| v == "1").unwrap_or(false) || is_broadcaster;
    let is_vip = badges.contains("vip/");
    let is_sub = tags.get("subscriber").map(|v| v == "1").unwrap_or(false)
        || badges.contains("subscriber/");
    Some(IncomingChat {
        user_id: tags.get("user-id").cloned().unwrap_or_default(),
        login: login.to_lowercase(),
        display: tags
            .get("display-name")
            .cloned()
            .unwrap_or_else(|| login.clone()),
        message: msg,
        is_mod,
        is_vip,
        is_sub,
        is_broadcaster,
        bits: tags
            .get("bits")
            .and_then(|b| b.parse::<u64>().ok())
            .unwrap_or(0),
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserNoticeEvent {
    pub kind: String,
    pub login: String,
    pub display: String,
    pub system_msg: String,
}

fn parse_usernotice(line: &str) -> Option<UserNoticeEvent> {
    if !line.contains(" USERNOTICE ") {
        return None;
    }
    let (tags, _) = if line.starts_with('@') {
        let (t, r) = line.split_once(' ')?;
        (parse_tags(t), r)
    } else {
        return None;
    };
    let msg_id = tags.get("msg-id")?.clone();
    let kind = match msg_id.as_str() {
        "sub" | "resub" | "subgift" | "submysterygift" | "anonsubgift"
        | "giftpaidupgrade" | "primepaidupgrade" | "anongiftpaidupgrade" => "subscribe",
        "raid" => "raid",
        "bits" | "cheer" | "bitsbadgetier" => "cheer",
        other => other,
    }
    .to_string();
    Some(UserNoticeEvent {
        kind,
        login: tags.get("login").cloned().unwrap_or_default(),
        display: tags
            .get("display-name")
            .cloned()
            .unwrap_or_else(|| tags.get("login").cloned().unwrap_or_default()),
        system_msg: tags.get("system-msg").cloned().unwrap_or_default(),
    })
}
