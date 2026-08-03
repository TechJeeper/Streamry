import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { useOpenFromQuery } from "../hooks/useOpenFromQuery";
import { ObsBrowserSourceBanner } from "./Media";
import type { Automation, MediaClip } from "../types";

const blank = (): Automation => ({
  id: "",
  name: "",
  triggerType: "subscribe",
  actionType: "chat",
  actionPayload: "Thanks for the sub, ${user}!",
  enabled: true,
  cooldownSecs: 0,
});

export function Automations() {
  const [items, setItems] = useState<Automation[]>([]);
  const [media, setMedia] = useState<MediaClip[]>([]);
  const [edit, setEdit] = useState<Automation | null>(null);

  async function load() {
    const [autos, clips] = await Promise.all([
      api.listAutomations(),
      api.listMedia().catch(() => [] as MediaClip[]),
    ]);
    setItems(autos);
    setMedia(clips);
  }
  useEffect(() => {
    load();
  }, []);

  const openEdit = useCallback((a: Automation) => setEdit(a), []);
  useOpenFromQuery(items, openEdit);

  function actionLabel(a: Automation) {
    if (a.actionType === "play_media") {
      const clip = media.find(
        (m) => m.id === a.actionPayload || m.name === a.actionPayload,
      );
      return `play media: ${clip?.name ?? a.actionPayload}`;
    }
    return `${a.actionType}: ${a.actionPayload}`;
  }

  function payloadLabel(actionType: string) {
    switch (actionType) {
      case "chat":
        return "Message — use ${user}";
      case "play_media":
        return "Media clip";
      default:
        return "Command or timer name";
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Automations</h1>
          <p>
            When something happens → chat, toggle a timer/command, or play media
            in OBS.
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setEdit(blank())}>
          New automation
        </button>
      </div>

      <ObsBrowserSourceBanner />

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">No automations yet.</div>
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th className="col-short">When</th>
                  <th>Then</th>
                  <th className="col-actions"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((a) => (
                  <tr key={a.id}>
                    <td>
                      <strong>{a.name}</strong>{" "}
                      <span className={`badge ${a.enabled ? "on" : "off"}`}>
                        {a.enabled ? "On" : "Off"}
                      </span>
                    </td>
                    <td>{a.triggerType}</td>
                    <td>{actionLabel(a)}</td>
                    <td className="col-actions">
                      <div className="btn-row">
                        <button className="btn btn-ghost" onClick={() => setEdit(a)}>
                          Edit
                        </button>
                        <button
                          className="btn btn-danger"
                          onClick={async () => {
                            await api.deleteAutomation(a.id);
                            load();
                          }}
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {edit && (
        <div className="modal-backdrop" onClick={() => setEdit(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{edit.id ? "Edit automation" : "New automation"}</h2>
            <div className="field">
              <label>Name</label>
              <input
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
              />
            </div>
            <div className="field">
              <label>When</label>
              <select
                value={edit.triggerType}
                onChange={(e) =>
                  setEdit({ ...edit, triggerType: e.target.value })
                }
              >
                <option value="subscribe">Subscribe / resub</option>
                <option value="raid">Raid</option>
                <option value="cheer">Cheer / bits</option>
                <option value="stream_online">Stream goes live</option>
                <option value="stream_offline">Stream goes offline</option>
              </select>
            </div>
            <div className="field">
              <label>Then</label>
              <select
                value={edit.actionType}
                onChange={(e) => {
                  const actionType = e.target.value;
                  let actionPayload = edit.actionPayload;
                  if (actionType === "play_media") {
                    actionPayload = media[0]?.id ?? "";
                  } else if (
                    edit.actionType === "play_media" &&
                    actionType === "chat"
                  ) {
                    actionPayload = "Thanks for the sub, ${user}!";
                  } else if (edit.actionType === "play_media") {
                    actionPayload = "";
                  }
                  setEdit({ ...edit, actionType, actionPayload });
                }}
              >
                <option value="chat">Send chat message</option>
                <option value="play_media">Play media (OBS)</option>
                <option value="enable_command">Enable command (by name)</option>
                <option value="disable_command">Disable command (by name)</option>
                <option value="enable_timer">Enable timer (by name)</option>
                <option value="disable_timer">Disable timer (by name)</option>
              </select>
            </div>
            <div className="field">
              <label>{payloadLabel(edit.actionType)}</label>
              {edit.actionType === "play_media" ? (
                <>
                  <select
                    value={edit.actionPayload}
                    onChange={(e) =>
                      setEdit({ ...edit, actionPayload: e.target.value })
                    }
                  >
                    <option value="">Select a clip…</option>
                    {media.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name} ({m.mediaType})
                      </option>
                    ))}
                  </select>
                  {media.length === 0 && (
                    <p className="field-hint">
                      Add clips under Media first, then pick one here.
                    </p>
                  )}
                </>
              ) : (
                <textarea
                  value={edit.actionPayload}
                  onChange={(e) =>
                    setEdit({ ...edit, actionPayload: e.target.value })
                  }
                />
              )}
            </div>
            <div className="field">
              <label>Enabled</label>
              <select
                value={edit.enabled ? "1" : "0"}
                onChange={(e) =>
                  setEdit({ ...edit, enabled: e.target.value === "1" })
                }
              >
                <option value="1">On</option>
                <option value="0">Off</option>
              </select>
            </div>
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertAutomation(edit);
                  setEdit(null);
                  load();
                }}
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
