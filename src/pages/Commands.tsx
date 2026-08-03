import { useEffect, useState } from "react";
import { api } from "../api";
import { Toggle } from "../components/Toggle";
import { ObsBrowserSourceBanner } from "./Media";
import type { ChatCommand, MediaClip } from "../types";

const blank = (): ChatCommand => ({
  id: "",
  name: "",
  aliases: "",
  response: "",
  enabled: true,
  permission: "everyone",
  globalCooldown: 5,
  userCooldown: 15,
  mediaId: null,
});

function copyFrom(c: ChatCommand): ChatCommand {
  return {
    ...c,
    id: "",
    name: c.name.trim() ? `${c.name}_copy` : "",
    enabled: true,
  };
}

export function Commands() {
  const [items, setItems] = useState<ChatCommand[]>([]);
  const [media, setMedia] = useState<MediaClip[]>([]);
  const [edit, setEdit] = useState<ChatCommand | null>(null);

  async function load() {
    const [cmds, clips] = await Promise.all([
      api.listCommands(),
      api.listMedia().catch(() => [] as MediaClip[]),
    ]);
    setItems(cmds);
    setMedia(clips);
  }
  useEffect(() => {
    load();
  }, []);

  async function toggleEnabled(cmd: ChatCommand, enabled: boolean) {
    const next = { ...cmd, enabled };
    setItems((list) => list.map((c) => (c.id === cmd.id ? next : c)));
    try {
      await api.upsertCommand(next);
    } catch {
      load();
    }
  }

  function mediaLabel(id?: string | null) {
    if (!id) return null;
    return media.find((m) => m.id === id)?.name ?? id;
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Commands</h1>
          <p>
            Chat triggers like !discord — reply in chat and/or play media in
            OBS.
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setEdit(blank())}>
          New command
        </button>
      </div>

      <ObsBrowserSourceBanner />

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">
            No commands yet. Add one or import from StreamElements in Settings.
          </div>
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th className="col-short">Command</th>
                  <th>Response</th>
                  <th className="col-media">Media</th>
                  <th className="col-short">Who</th>
                  <th className="col-check">On</th>
                  <th className="col-actions"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((c) => (
                  <tr key={c.id}>
                    <td>
                      <strong>!{c.name}</strong>
                    </td>
                    <td>{c.response || "—"}</td>
                    <td>{mediaLabel(c.mediaId) ?? "—"}</td>
                    <td>{c.permission}</td>
                    <td>
                      <Toggle
                        checked={c.enabled}
                        label={`Toggle !${c.name}`}
                        onChange={(enabled) => toggleEnabled(c, enabled)}
                      />
                    </td>
                    <td className="col-actions">
                      <div className="btn-row">
                        <button
                          className="btn btn-ghost"
                          onClick={() => setEdit(c)}
                        >
                          Edit
                        </button>
                        <button
                          className="btn btn-ghost"
                          onClick={() => setEdit(copyFrom(c))}
                        >
                          Copy
                        </button>
                        <button
                          className="btn btn-danger"
                          onClick={async () => {
                            await api.deleteCommand(c.id);
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
            <h2>{edit.id ? "Edit command" : "New command"}</h2>
            <div className="field">
              <label>Name (without !)</label>
              <input
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
              />
            </div>
            <div className="field">
              <label>Aliases (comma-separated)</label>
              <input
                value={edit.aliases}
                onChange={(e) => setEdit({ ...edit, aliases: e.target.value })}
              />
            </div>
            <div className="field">
              <label>
                Chat response — use ${"{user}"}, ${"{channel}"} (optional if
                media is set)
              </label>
              <textarea
                value={edit.response}
                onChange={(e) => setEdit({ ...edit, response: e.target.value })}
              />
            </div>
            <div className="field">
              <label>Play media in OBS</label>
              <select
                value={edit.mediaId ?? ""}
                onChange={(e) =>
                  setEdit({
                    ...edit,
                    mediaId: e.target.value || null,
                  })
                }
              >
                <option value="">None</option>
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
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Permission</label>
                <select
                  value={edit.permission}
                  onChange={(e) =>
                    setEdit({ ...edit, permission: e.target.value })
                  }
                >
                  <option value="everyone">Everyone</option>
                  <option value="sub">Subscriber+</option>
                  <option value="vip">VIP+</option>
                  <option value="mod">Mod+</option>
                  <option value="broadcaster">Broadcaster</option>
                </select>
              </div>
              <div className="field">
                <label>Enabled</label>
                <div style={{ paddingTop: 6 }}>
                  <Toggle
                    checked={edit.enabled}
                    label="Enabled"
                    onChange={(enabled) => setEdit({ ...edit, enabled })}
                  />
                </div>
              </div>
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Global cooldown (sec)</label>
                <input
                  type="number"
                  value={edit.globalCooldown}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      globalCooldown: Number(e.target.value) || 0,
                    })
                  }
                />
              </div>
              <div className="field">
                <label>User cooldown (sec)</label>
                <input
                  type="number"
                  value={edit.userCooldown}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      userCooldown: Number(e.target.value) || 0,
                    })
                  }
                />
              </div>
            </div>
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertCommand(edit);
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
