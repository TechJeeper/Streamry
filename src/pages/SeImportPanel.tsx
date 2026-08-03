import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { SePreview } from "../types";

const EXPORT_STREAM = "https://export.stream/";
const SE_CHANNELS = "https://streamelements.com/dashboard/account/channels";

export function SeImportPanel() {
  const [path, setPath] = useState("");
  const [preview, setPreview] = useState<SePreview | null>(null);
  const [selCmds, setSelCmds] = useState<Set<string>>(new Set());
  const [selTimers, setSelTimers] = useState<Set<string>>(new Set());
  const [selVars, setSelVars] = useState<Set<string>>(new Set());
  const [selAutos, setSelAutos] = useState<Set<string>>(new Set());
  const [collision, setCollision] = useState("skip");
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  async function pick() {
    setErr("");
    setMsg("");
    const file = await open({
      multiple: false,
      filters: [{ name: "StreamElements export", extensions: ["zip"] }],
    });
    if (!file || Array.isArray(file)) return;
    setPath(file);
    try {
      const p = await api.parseSeZip(file);
      setPreview({
        commands: p.commands ?? [],
        timers: p.timers ?? [],
        variables: p.variables ?? [],
        automations: p.automations ?? [],
      });
      setSelCmds(new Set((p.commands ?? []).map((c) => c.id)));
      setSelTimers(new Set((p.timers ?? []).map((t) => t.id)));
      setSelVars(new Set((p.variables ?? []).map((v) => v.id)));
      setSelAutos(new Set((p.automations ?? []).map((a) => a.id)));
    } catch (e) {
      setErr(String(e));
      setPreview(null);
    }
  }

  function toggle(set: Set<string>, id: string, setter: (s: Set<string>) => void) {
    const next = new Set(set);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setter(next);
  }

  return (
    <div className="panel" style={{ marginTop: 16 }}>
      <div className="page-head" style={{ marginBottom: 12 }}>
        <div>
          <h3 style={{ marginTop: 0, marginBottom: 6 }}>StreamElements Import</h3>
          <p style={{ margin: 0, color: "var(--muted)" }}>
            Import commands, timers, variables, and chat-alert automations from an
            export.stream ZIP.
          </p>
        </div>
        <div className="btn-row">
          <button
            className="btn btn-ghost"
            onClick={() => openUrl(EXPORT_STREAM)}
          >
            Open export.stream
          </button>
          <button className="btn btn-primary" onClick={pick}>
            Choose ZIP
          </button>
        </div>
      </div>

      {err && <p style={{ color: "var(--danger)" }}>{err}</p>}
      {msg && <p style={{ color: "var(--ok)" }}>{msg}</p>}

      {!preview ? (
        <>
          <p className="hint" style={{ marginBottom: 14 }}>
            StreamElements doesn’t offer a built-in download for bot config. Use{" "}
            <button
              type="button"
              className="linkish"
              onClick={() => openUrl(EXPORT_STREAM)}
            >
              export.stream
            </button>{" "}
            to create a ZIP, then import it here.
          </p>
          <ol className="guide-list">
            <li>
              Open{" "}
              <button
                type="button"
                className="linkish"
                onClick={() => openUrl(EXPORT_STREAM)}
              >
                export.stream
              </button>
              .
            </li>
            <li>
              Connect your StreamElements account, or paste your JWT from{" "}
              <button
                type="button"
                className="linkish"
                onClick={() => openUrl(SE_CHANNELS)}
              >
                Account → Channels
              </button>{" "}
              (Show secrets).
            </li>
            <li>
              Select <strong>Commands</strong>, <strong>Timers</strong>, and{" "}
              <strong>Variables</strong> (or <strong>Bot Config</strong> /{" "}
              <strong>All</strong>).
            </li>
            <li>
              Click <strong>Export</strong>, download the ZIP, then{" "}
              <strong>Choose ZIP</strong> here.
            </li>
          </ol>
          <p className="hint">
            Chat-alert automations (sub/raid/cheer/live) import when the ZIP
            includes module/alert JSON. export.stream always includes variables
            when you select them; overlays and loyalty are ignored.
          </p>
        </>
      ) : (
        <>
          <div className="btn-row" style={{ marginBottom: 12, flexWrap: "wrap" }}>
            <button
              className="btn btn-ghost"
              onClick={() =>
                setSelCmds(new Set(preview.commands.map((c) => c.id)))
              }
            >
              All commands
            </button>
            <button
              className="btn btn-ghost"
              onClick={() =>
                setSelTimers(new Set(preview.timers.map((t) => t.id)))
              }
            >
              All timers
            </button>
            <button
              className="btn btn-ghost"
              onClick={() =>
                setSelVars(new Set(preview.variables.map((v) => v.id)))
              }
            >
              All variables
            </button>
            <button
              className="btn btn-ghost"
              onClick={() =>
                setSelAutos(new Set(preview.automations.map((a) => a.id)))
              }
            >
              All automations
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => {
                setSelCmds(new Set());
                setSelTimers(new Set());
                setSelVars(new Set());
                setSelAutos(new Set());
              }}
            >
              None
            </button>
          </div>
          <div className="field">
            <label>If a command or variable name already exists</label>
            <select
              value={collision}
              onChange={(e) => setCollision(e.target.value)}
            >
              <option value="skip">Skip</option>
              <option value="overwrite">Overwrite</option>
            </select>
          </div>
          <button
            className="btn btn-accent"
            style={{ marginBottom: 16 }}
            onClick={async () => {
              try {
                const result = await api.importSe(
                  path,
                  [...selCmds],
                  [...selTimers],
                  [...selVars],
                  [...selAutos],
                  collision,
                );
                setMsg(
                  `Imported ${result.importedCommands} commands, ${result.importedTimers} timers, ${result.importedVariables} variables, ${result.importedAutomations} automations (skipped ${result.skipped}).`,
                );
              } catch (e) {
                setErr(String(e));
              }
            }}
          >
            Import selected
          </button>

          <div className="grid-2">
            <div>
              <h4 style={{ marginTop: 0 }}>Commands ({preview.commands.length})</h4>
              <div className="check-list">
                {preview.commands.length === 0 ? (
                  <p className="hint">None in this ZIP.</p>
                ) : (
                  preview.commands.map((c) => (
                    <label key={c.id} className="check-item">
                      <input
                        type="checkbox"
                        checked={selCmds.has(c.id)}
                        onChange={() => toggle(selCmds, c.id, setSelCmds)}
                      />
                      <span>
                        <strong>!{c.name}</strong>
                        <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                          {c.response}
                        </div>
                      </span>
                    </label>
                  ))
                )}
              </div>
            </div>
            <div>
              <h4 style={{ marginTop: 0 }}>Timers ({preview.timers.length})</h4>
              <div className="check-list">
                {preview.timers.length === 0 ? (
                  <p className="hint">None in this ZIP.</p>
                ) : (
                  preview.timers.map((t) => (
                    <label key={t.id} className="check-item">
                      <input
                        type="checkbox"
                        checked={selTimers.has(t.id)}
                        onChange={() => toggle(selTimers, t.id, setSelTimers)}
                      />
                      <span>
                        <strong>{t.name}</strong> · every {t.interval}m
                        <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                          {t.message}
                        </div>
                      </span>
                    </label>
                  ))
                )}
              </div>
            </div>
            <div>
              <h4 style={{ marginTop: 0 }}>
                Variables ({preview.variables.length})
              </h4>
              <div className="check-list">
                {preview.variables.length === 0 ? (
                  <p className="hint">None in this ZIP.</p>
                ) : (
                  preview.variables.map((v) => (
                    <label key={v.id} className="check-item">
                      <input
                        type="checkbox"
                        checked={selVars.has(v.id)}
                        onChange={() => toggle(selVars, v.id, setSelVars)}
                      />
                      <span>
                        <strong>${`{${v.name}}`}</strong>
                        <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                          {v.value}
                        </div>
                      </span>
                    </label>
                  ))
                )}
              </div>
            </div>
            <div>
              <h4 style={{ marginTop: 0 }}>
                Automations ({preview.automations.length})
              </h4>
              <div className="check-list">
                {preview.automations.length === 0 ? (
                  <p className="hint">
                    No chat-alert modules found in this ZIP. export.stream does
                    not include Chat Alerts yet — add those under Automations
                    manually if needed.
                  </p>
                ) : (
                  preview.automations.map((a) => (
                    <label key={a.id} className="check-item">
                      <input
                        type="checkbox"
                        checked={selAutos.has(a.id)}
                        onChange={() => toggle(selAutos, a.id, setSelAutos)}
                      />
                      <span>
                        <strong>{a.name}</strong> · {a.triggerType}
                        <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                          {a.actionPayload}
                        </div>
                      </span>
                    </label>
                  ))
                )}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
