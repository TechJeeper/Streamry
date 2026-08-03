import { useEffect, useState } from "react";
import { api } from "../api";
import { Toggle } from "../components/Toggle";
import type { ChatTimer } from "../types";

const blank = (): ChatTimer => ({
  id: "",
  name: "",
  message: "",
  intervalMins: 10,
  minChatLines: 0,
  enabled: true,
  liveOnly: false,
});

function copyFrom(t: ChatTimer): ChatTimer {
  return {
    ...t,
    id: "",
    name: t.name.trim() ? `${t.name} (copy)` : "Timer (copy)",
    enabled: true,
  };
}

export function Timers() {
  const [items, setItems] = useState<ChatTimer[]>([]);
  const [edit, setEdit] = useState<ChatTimer | null>(null);

  async function load() {
    setItems(await api.listTimers());
  }
  useEffect(() => {
    load();
  }, []);

  async function toggleEnabled(timer: ChatTimer, enabled: boolean) {
    const next = { ...timer, enabled };
    setItems((list) => list.map((t) => (t.id === timer.id ? next : t)));
    try {
      await api.upsertTimer(next);
    } catch {
      load();
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Timers</h1>
          <p>Automatic chat messages on an interval.</p>
        </div>
        <button className="btn btn-primary" onClick={() => setEdit(blank())}>
          New timer
        </button>
      </div>

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">No timers yet.</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Every</th>
                <th>Message</th>
                <th>On</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {items.map((t) => (
                <tr key={t.id}>
                  <td>
                    <strong>{t.name}</strong>
                  </td>
                  <td>{t.intervalMins} min</td>
                  <td>{t.message}</td>
                  <td>
                    <Toggle
                      checked={t.enabled}
                      label={`Toggle ${t.name}`}
                      onChange={(enabled) => toggleEnabled(t, enabled)}
                    />
                  </td>
                  <td>
                    <div className="btn-row">
                      <button className="btn btn-ghost" onClick={() => setEdit(t)}>
                        Edit
                      </button>
                      <button
                        className="btn btn-ghost"
                        onClick={() => setEdit(copyFrom(t))}
                      >
                        Copy
                      </button>
                      <button
                        className="btn btn-danger"
                        onClick={async () => {
                          await api.deleteTimer(t.id);
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
        )}
      </div>

      {edit && (
        <div className="modal-backdrop" onClick={() => setEdit(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{edit.id ? "Edit timer" : "New timer"}</h2>
            <div className="field">
              <label>Name</label>
              <input
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
              />
            </div>
            <div className="field">
              <label>Message</label>
              <textarea
                value={edit.message}
                onChange={(e) => setEdit({ ...edit, message: e.target.value })}
              />
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Interval (minutes)</label>
                <input
                  type="number"
                  value={edit.intervalMins}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      intervalMins: Number(e.target.value) || 1,
                    })
                  }
                />
              </div>
              <div className="field">
                <label>Min chat lines</label>
                <input
                  type="number"
                  value={edit.minChatLines}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      minChatLines: Number(e.target.value) || 0,
                    })
                  }
                />
              </div>
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Only while live</label>
                <select
                  value={edit.liveOnly ? "1" : "0"}
                  onChange={(e) =>
                    setEdit({ ...edit, liveOnly: e.target.value === "1" })
                  }
                >
                  <option value="0">No</option>
                  <option value="1">Yes</option>
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
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertTimer(edit);
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
