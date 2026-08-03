import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { ActiveGiveaway, RuntimeStatus } from "../types";

export function Dashboard({
  status,
  refreshStatus,
}: {
  status: RuntimeStatus | null;
  refreshStatus: () => void;
}) {
  const [active, setActive] = useState<ActiveGiveaway | null>(null);
  const [chat, setChat] = useState<{ user: string; message: string }[]>([]);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  async function loadGw() {
    setActive(await api.getActiveGiveaway());
  }

  useEffect(() => {
    loadGw();
    const unsubs = [
      listen("giveaway-updated", () => loadGw()),
      listen<{ user: string; message: string }>("chat-message", (e) => {
        setChat((c) => [...c.slice(-40), e.payload]);
      }),
      listen("status-changed", () => refreshStatus()),
    ];
    return () => unsubs.forEach((p) => p.then((u) => u()));
  }, [refreshStatus]);

  async function toggleConnect() {
    setErr("");
    try {
      if (status?.connected) await api.disconnectBot();
      else await api.connectBot();
      refreshStatus();
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Dashboard</h1>
          <p>Connection, giveaways, and a quick peek at chat.</p>
        </div>
        <button className="btn btn-primary" onClick={toggleConnect}>
          {status?.connected ? "Disconnect" : "Connect bot"}
        </button>
      </div>

      {err && <p style={{ color: "var(--danger)" }}>{err}</p>}

      <div className="grid-3" style={{ marginBottom: 16 }}>
        <div className="panel stat">
          <div className="label">Status</div>
          <div className="value">
            {status?.connected ? "Connected" : "Offline"}
          </div>
        </div>
        <div className="panel stat">
          <div className="label">Channel</div>
          <div className="value">#{status?.channel || "—"}</div>
        </div>
        <div className="panel stat">
          <div className="label">Stream</div>
          <div className="value">{status?.live ? "LIVE" : "Offline"}</div>
        </div>
      </div>

      <div className="grid-2">
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Active giveaway</h3>
          {active ? (
            <>
              <p>
                <strong>{active.giveaway.title}</strong> — {active.entryCount}{" "}
                entered
              </p>
              <p style={{ color: "var(--muted)" }}>
                Type <code>{active.giveaway.entryCommand}</code> to enter · draw
                with <code>{active.giveaway.drawCommand}</code>
              </p>
              <div className="btn-row">
                <button
                  className="btn btn-accent"
                  onClick={async () => {
                    await api.drawGiveaway();
                    loadGw();
                  }}
                >
                  Pick winner
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={async () => {
                    await api.stopGiveaway();
                    loadGw();
                  }}
                >
                  Stop
                </button>
              </div>
            </>
          ) : (
            <p className="empty">No giveaway running. Start one from Giveaways.</p>
          )}
        </div>

        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Say something</h3>
          <div className="field">
            <input
              value={msg}
              onChange={(e) => setMsg(e.target.value)}
              placeholder="Send a chat message as the bot"
              onKeyDown={async (e) => {
                if (e.key === "Enter" && msg.trim()) {
                  await api.sendChat(msg.trim());
                  setMsg("");
                }
              }}
            />
          </div>
          <button
            className="btn btn-primary"
            disabled={!status?.connected || !msg.trim()}
            onClick={async () => {
              await api.sendChat(msg.trim());
              setMsg("");
            }}
          >
            Send
          </button>
          <div className="chat-log" style={{ marginTop: 14 }}>
            {chat.length === 0 && (
              <div style={{ color: "var(--muted)" }}>Chat will appear here.</div>
            )}
            {chat.map((c, i) => (
              <div key={i}>
                <strong>{c.user}</strong>: {c.message}
              </div>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
