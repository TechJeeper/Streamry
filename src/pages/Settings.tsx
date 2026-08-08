import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { CollapsiblePanel } from "../components/CollapsiblePanel";
import { applyTheme } from "../theme";
import type { AppSettings, DeviceCode, UpdateCheck } from "../types";
import { SeImportPanel } from "./SeImportPanel";
import { StreamDeckPanel } from "./StreamDeckPanel";

const SCOPES = [
  "chat:read",
  "chat:edit",
  "user:read:chat",
  "channel:read:subscriptions",
  "channel:read:ads",
  "moderator:read:followers",
];

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [adsDevice, setAdsDevice] = useState<DeviceCode | null>(null);
  const [authBusy, setAuthBusy] = useState(false);
  const [adsAuthBusy, setAdsAuthBusy] = useState(false);
  const [resetStep, setResetStep] = useState<0 | 1 | 2>(0);
  const [resetConfirm, setResetConfirm] = useState("");
  const [resetBusy, setResetBusy] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [updateInfo, setUpdateInfo] = useState<UpdateCheck | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateMsg, setUpdateMsg] = useState("");

  useEffect(() => {
    api.getSettings().then((s) => {
      setSettings(s);
      applyTheme(s.theme || "dark");
    });
    isEnabled().then(setAutostart).catch(() => {});
    api.getAppVersion().then(setAppVersion).catch(() => {});
    api
      .checkForUpdate()
      .then((check) => {
        if (check.updateAvailable) setUpdateInfo(check);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const unsubs = [
      listen("auth-success", async () => {
        const s = await api.getSettings();
        setSettings(s);
        setDevice(null);
        setAuthBusy(false);
        setErr("");
        setMsg(
          s.accountMode === "bot" &&
            s.botLogin &&
            s.channel &&
            s.botLogin.toLowerCase() === s.channel.toLowerCase()
            ? `Connected as ${s.botLogin} — that matches your channel. For a separate bot, log out of Twitch in the browser, sign in as the bot account, then connect again.`
            : `Connected as ${s.botLogin || "Twitch user"}.`,
        );
        try {
          await api.connectBot();
        } catch {
          /* optional */
        }
      }),
      listen<string>("auth-error", (e) => {
        setErr(String(e.payload));
        setAuthBusy(false);
        setDevice(null);
      }),
      listen("ads-auth-success", async () => {
        const s = await api.getSettings();
        setSettings(s);
        setAdsDevice(null);
        setAdsAuthBusy(false);
        setErr("");
        setMsg(
          `Streamer ads authorized for ${s.channel}. Reconnect the bot so ad triggers arm.`,
        );
        try {
          await api.disconnectBot();
          await api.connectBot();
        } catch {
          /* optional */
        }
      }),
      listen<string>("ads-auth-error", (e) => {
        setErr(String(e.payload));
        setAdsAuthBusy(false);
        setAdsDevice(null);
      }),
    ];
    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  if (!settings) return null;

  async function persist(next: AppSettings) {
    setSettings(next);
    await api.saveSettings(next);
  }

  async function startReconnect() {
    if (!settings) return;
    setAuthBusy(true);
    setErr("");
    setMsg("");
    try {
      await persist(settings);
      const d = await api.startDeviceLogin(SCOPES);
      setDevice(d);
      await openUrl(d.verificationUri);
    } catch (e) {
      setErr(String(e));
      setAuthBusy(false);
    }
  }

  async function startStreamerAdsAuth() {
    if (!settings) return;
    setAdsAuthBusy(true);
    setErr("");
    setMsg("");
    try {
      await persist(settings);
      const d = await api.startStreamerAdsLogin();
      setAdsDevice(d);
      await openUrl(d.verificationUri);
    } catch (e) {
      setErr(String(e));
      setAdsAuthBusy(false);
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Settings</h1>
          <p>Twitch connection, startup, Stream Deck, StreamElements import, backups, and reset.</p>
        </div>
      </div>

      {msg && <p style={{ color: "var(--ok)" }}>{msg}</p>}
      {err && <p style={{ color: "var(--danger)" }}>{err}</p>}

      <div className="grid-2">
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Twitch</h3>
          <div className="field">
            <label>Client ID</label>
            <input
              value={settings.clientId}
              onChange={(e) =>
                setSettings({ ...settings, clientId: e.target.value })
              }
            />
          </div>
          <p className="hint" style={{ marginTop: -8, marginBottom: 14 }}>
            Need one? Create a <strong>Public</strong> app named Streamry,
            set redirect URL to <code>https://localhost</code>, then copy Client
            ID (not the secret) from{" "}
            <a
              href="https://dev.twitch.tv/console/apps"
              target="_blank"
              rel="noreferrer"
            >
              Twitch Developer Console
            </a>
            .
          </p>
          <div className="field">
            <label>Channel</label>
            <input
              value={settings.channel}
              onChange={(e) =>
                setSettings({ ...settings, channel: e.target.value })
              }
            />
          </div>
          <div className="field">
            <label>Bot login</label>
            <input
              value={settings.botLogin}
              onChange={(e) =>
                setSettings({ ...settings, botLogin: e.target.value })
              }
            />
          </div>
          <div className="field">
            <label>Account mode</label>
            <select
              value={settings.accountMode}
              onChange={(e) =>
                setSettings({ ...settings, accountMode: e.target.value })
              }
            >
              <option value="streamer">Streamer account</option>
              <option value="bot">Separate bot account</option>
            </select>
          </div>
          <div className="btn-row">
            <button
              className="btn btn-primary"
              onClick={async () => {
                await persist(settings);
                setMsg("Settings saved.");
              }}
            >
              Save
            </button>
            <button
              className="btn btn-accent"
              disabled={authBusy || !settings.clientId}
              onClick={startReconnect}
            >
              {device ? "Waiting…" : "Connect with Twitch"}
            </button>
            <button
              className="btn btn-ghost"
              onClick={async () => {
                await api.logout();
                setDevice(null);
                setMsg("Logged out. Use Connect with Twitch to sign in again.");
              }}
            >
              Log out
            </button>
          </div>
          {settings.accountMode === "bot" && (
            <p className="hint" style={{ marginTop: 12 }}>
              Separate bot mode: authorize while logged into Twitch as your{" "}
              <strong>bot</strong> account — not {settings.channel || "your channel"}.
              Switch accounts in the browser first if needed.
            </p>
          )}
          {settings.accountMode === "bot" && (
            <div style={{ marginTop: 16 }}>
              <h4 style={{ margin: "0 0 8px" }}>Ad break triggers</h4>
              <p className="hint" style={{ marginTop: 0 }}>
                Twitch only allows the <strong>streamer</strong> account to
                listen for midroll ads. Authorize {settings.channel || "your channel"}{" "}
                separately (bot stays used for chat).
              </p>
              <div className="btn-row">
                <button
                  className="btn btn-accent"
                  disabled={adsAuthBusy || !settings.clientId || !settings.channel}
                  onClick={startStreamerAdsAuth}
                >
                  {adsDevice
                    ? "Waiting…"
                    : settings.streamerAdsAuthorized
                      ? "Re-authorize streamer for ads"
                      : "Authorize streamer for ads"}
                </button>
              </div>
              {settings.streamerAdsAuthorized && !adsDevice && (
                <p className="hint" style={{ marginTop: 8, color: "var(--ok)" }}>
                  Streamer ads authorization is saved.
                </p>
              )}
              {adsDevice && (
                <div style={{ marginTop: 14 }}>
                  <p className="hint" style={{ marginBottom: 8 }}>
                    Log into Twitch as <strong>{settings.channel}</strong>, enter
                    this code, then Authorize.
                  </p>
                  <div className="code-box">{adsDevice.userCode}</div>
                  <div className="btn-row" style={{ marginTop: 10 }}>
                    <button
                      className="btn btn-ghost"
                      onClick={() => openUrl(adsDevice.verificationUri)}
                    >
                      Open Twitch again
                    </button>
                    <button
                      className="btn btn-ghost"
                      onClick={() => {
                        setAdsDevice(null);
                        setAdsAuthBusy(false);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
          {settings.accountMode !== "bot" && (
            <p className="hint" style={{ marginTop: 12 }}>
              Ad break automations need a fresh Twitch connect after updating —
              use <strong>Connect with Twitch</strong> so{" "}
              <code>channel:read:ads</code> is granted. Only midroll ads fire
              (Run Ad / Ads Manager), not every preroll.
            </p>
          )}
          {device && (
            <div style={{ marginTop: 14 }}>
              <p className="hint" style={{ marginBottom: 8 }}>
                Enter this code on Twitch, then click <strong>Authorize</strong>.
              </p>
              <div className="code-box">{device.userCode}</div>
              <div className="btn-row" style={{ marginTop: 10 }}>
                <button
                  className="btn btn-ghost"
                  onClick={() => openUrl(device.verificationUri)}
                >
                  Open Twitch again
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={() => {
                    setDevice(null);
                    setAuthBusy(false);
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="panel">
          <h3 style={{ marginTop: 0 }}>App</h3>
          <label style={{ display: "flex", gap: 10, marginBottom: 16 }}>
            <input
              type="checkbox"
              checked={autostart}
              onChange={async (e) => {
                try {
                  if (e.target.checked) await enable();
                  else await disable();
                  setAutostart(e.target.checked);
                } catch (ex) {
                  setErr(String(ex));
                }
              }}
            />
            Start Streamry when I sign in
          </label>
          <label style={{ display: "flex", gap: 10, marginBottom: 16 }}>
            <input
              type="checkbox"
              checked={settings.timersLiveOnly}
              onChange={(e) =>
                persist({ ...settings, timersLiveOnly: e.target.checked })
              }
            />
            Run timers only while live
          </label>
          <label style={{ display: "flex", gap: 10, marginBottom: 16 }}>
            <input
              type="checkbox"
              checked={(settings.theme || "dark") === "light"}
              onChange={(e) => {
                const theme = e.target.checked ? "light" : "dark";
                applyTheme(theme);
                persist({ ...settings, theme });
              }}
            />
            Light mode
          </label>
          <p style={{ color: "var(--muted)", fontSize: "0.9rem" }}>
            Closing the window minimizes to the system tray — your bot keeps
            running. Use Quit from the tray menu to exit. The bot connects
            automatically when Streamry starts.
          </p>
          <hr
            style={{
              border: "none",
              borderTop: "1px solid var(--line)",
              margin: "18px 0 14px",
            }}
          />
          <p style={{ margin: "0 0 10px", color: "var(--muted)", fontSize: "0.9rem" }}>
            Version {appVersion || "…"}
          </p>
          {updateInfo?.updateAvailable ? (
            <div style={{ marginBottom: 12 }}>
              <p style={{ margin: "0 0 8px", color: "var(--ok)" }}>
                Update available: <strong>{updateInfo.latestVersion}</strong>
              </p>
              {updateInfo.notes && (
                <p
                  style={{
                    margin: "0 0 10px",
                    color: "var(--muted)",
                    fontSize: "0.9rem",
                  }}
                >
                  {updateInfo.notes}
                </p>
              )}
              <div className="btn-row">
                <button
                  className="btn btn-primary"
                  onClick={() => openUrl(updateInfo.downloadUrl)}
                >
                  Go to downloads
                </button>
              </div>
            </div>
          ) : (
            updateMsg && (
              <p style={{ margin: "0 0 10px", color: "var(--muted)", fontSize: "0.9rem" }}>
                {updateMsg}
              </p>
            )
          )}
          <button
            className="btn btn-ghost"
            disabled={updateBusy}
            onClick={async () => {
              setUpdateBusy(true);
              setUpdateMsg("");
              setErr("");
              try {
                const check = await api.checkForUpdate();
                setUpdateInfo(check);
                if (!check.updateAvailable) {
                  setUpdateMsg(`You’re up to date (${check.currentVersion}).`);
                }
              } catch (e) {
                setErr(String(e));
                setUpdateInfo(null);
              } finally {
                setUpdateBusy(false);
              }
            }}
          >
            {updateBusy ? "Checking…" : "Check for updates"}
          </button>
        </div>
      </div>

      <SeImportPanel />

      {settings && (
        <StreamDeckPanel
          settings={settings}
          onSettings={(next) => setSettings(next)}
        />
      )}

      <CollapsiblePanel
        title="Backup & restore"
        blurb="Exports commands, timers, giveaways, automations, and variables (not login tokens)."
      >
        <div className="btn-row">
          <button
            className="btn btn-accent"
            onClick={async () => {
              const path = await save({
                filters: [
                  { name: "Streamry backup", extensions: ["streamry"] },
                ],
                defaultPath: "backup.streamry",
              });
              if (!path) return;
              await api.exportBackup(path);
              setMsg("Backup saved.");
            }}
          >
            Backup now
          </button>
          <button
            className="btn btn-ghost"
            onClick={async () => {
              const file = await open({
                filters: [
                  {
                    name: "Streamry backup",
                    extensions: ["streamry", "zip"],
                  },
                ],
              });
              if (!file || Array.isArray(file)) return;
              const preview = await api.previewBackup(file);
              const ok = window.confirm(
                `Restore ${preview.commands} commands, ${preview.timers} timers, ${preview.giveaways} giveaways, ${preview.automations} automations, ${preview.variables ?? 0} variables?`,
              );
              if (!ok) return;
              await api.restoreBackup({
                path: file,
                includeCommands: true,
                includeTimers: true,
                includeGiveaways: true,
                includeAutomations: true,
                includeVariables: true,
                replace: false,
              });
              setMsg("Backup restored.");
            }}
          >
            Restore…
          </button>
        </div>
      </CollapsiblePanel>

      <CollapsiblePanel
        title="Reset"
        blurb="Erase all Streamry data on this computer and return to setup. This cannot be undone."
      >
        <div className="btn-row">
          <button
            className="btn btn-danger"
            onClick={() => {
              setResetConfirm("");
              setResetStep(1);
            }}
          >
            Reset Streamry…
          </button>
        </div>
      </CollapsiblePanel>

      {resetStep > 0 && (
        <div
          className="modal-backdrop"
          onClick={() => {
            if (!resetBusy) setResetStep(0);
          }}
        >
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            {resetStep === 1 ? (
              <>
                <h2>Reset Streamry?</h2>
                <p style={{ color: "var(--muted)", marginTop: 0 }}>
                  This permanently deletes local bot data and logs you out of
                  Twitch. Export a backup first if you want to keep anything.
                </p>
                <ul
                  style={{
                    color: "var(--muted)",
                    margin: "0 0 18px",
                    paddingLeft: 18,
                    lineHeight: 1.55,
                  }}
                >
                  <li>Commands, timers, giveaways, and winner history</li>
                  <li>Automations, variables, and media clips</li>
                  <li>App settings and Twitch connection</li>
                </ul>
                <div className="btn-row">
                  <button
                    className="btn btn-ghost"
                    onClick={() => setResetStep(0)}
                  >
                    Cancel
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={() => {
                      setResetConfirm("");
                      setResetStep(2);
                    }}
                  >
                    Continue
                  </button>
                </div>
              </>
            ) : (
              <>
                <h2>Confirm reset</h2>
                <p style={{ color: "var(--muted)", marginTop: 0 }}>
                  Type <strong>RESET</strong> to confirm. This cannot be undone.
                </p>
                <div className="field">
                  <label>Confirmation</label>
                  <input
                    value={resetConfirm}
                    onChange={(e) => setResetConfirm(e.target.value)}
                    placeholder="RESET"
                    autoFocus
                    disabled={resetBusy}
                  />
                </div>
                <div className="btn-row">
                  <button
                    className="btn btn-ghost"
                    disabled={resetBusy}
                    onClick={() => setResetStep(0)}
                  >
                    Cancel
                  </button>
                  <button
                    className="btn btn-danger"
                    disabled={
                      resetBusy || resetConfirm.trim().toUpperCase() !== "RESET"
                    }
                    onClick={async () => {
                      setResetBusy(true);
                      setErr("");
                      try {
                        await api.resetApp();
                        window.location.reload();
                      } catch (e) {
                        setErr(String(e));
                        setResetBusy(false);
                        setResetStep(0);
                      }
                    }}
                  >
                    {resetBusy ? "Resetting…" : "Erase everything"}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
}
