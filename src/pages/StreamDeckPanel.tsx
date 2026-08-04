import { useEffect, useState } from "react";
import { api } from "../api";
import { CollapsiblePanel } from "../components/CollapsiblePanel";
import type { AppSettings, StreamDeckStatus } from "../types";

type Props = {
  settings: AppSettings;
  onSettings: (next: AppSettings) => void;
};

export function StreamDeckPanel({ settings, onSettings }: Props) {
  const [status, setStatus] = useState<StreamDeckStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  async function refresh() {
    try {
      const s = await api.getStreamDeckStatus();
      setStatus(s);
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function install() {
    setBusy(true);
    setErr("");
    setMsg("");
    try {
      const s = await api.installStreamDeckPlugin();
      setStatus(s);
      const next = await api.getSettings();
      onSettings(next);
      setMsg(
        "Stream Deck plugin installed. Restart Stream Deck if Streamry actions don’t appear in the action list.",
      );
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function toggleControl(enabled: boolean) {
    setErr("");
    setMsg("");
    try {
      const s = await api.setStreamDeckControl(enabled);
      setStatus(s);
      const next = await api.getSettings();
      onSettings(next);
      setMsg(enabled ? "Control API enabled." : "Control API disabled.");
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <CollapsiblePanel
      title="Stream Deck"
      blurb="Install the Streamry plugin so Stream Deck can run commands, automations, giveaways, and media."
    >
      {status && (
        <p style={{ color: "var(--muted)", marginTop: 0, lineHeight: 1.5 }}>
          {status.message}
          {status.installed && status.installPath ? (
            <>
              <br />
              <span style={{ fontSize: 12, opacity: 0.85 }}>{status.installPath}</span>
            </>
          ) : null}
        </p>
      )}

      <label
        className="check-row"
        style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}
      >
        <input
          type="checkbox"
          checked={!!settings.streamDeckControlEnabled}
          disabled={!status?.supported || busy}
          onChange={(e) => toggleControl(e.target.checked)}
        />
        <span>
          Enable control API
          {status ? ` (port ${status.controlPort})` : ""}
        </span>
      </label>

      <div className="btn-row">
        <button
          className="btn btn-accent"
          disabled={!status?.supported || busy}
          onClick={install}
        >
          {busy ? "Installing…" : "Install StreamDeck Integration"}
        </button>
        <button className="btn btn-ghost" disabled={busy} onClick={refresh}>
          Refresh status
        </button>
      </div>

      {msg && <p style={{ color: "var(--ok)" }}>{msg}</p>}
      {err && <p style={{ color: "var(--danger)" }}>{err}</p>}
    </CollapsiblePanel>
  );
}
