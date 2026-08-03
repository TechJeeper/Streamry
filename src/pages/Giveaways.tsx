import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { ActiveGiveaway, Giveaway, GiveawayRunHistory } from "../types";

const blank = (): Giveaway => ({
  id: "",
  title: "Giveaway",
  prize: "",
  entryCommand: "!enter",
  drawCommand: "!pickwinner",
  durationMins: 5,
  winnerCount: 1,
  eligibility: "everyone",
  excludeMods: false,
  confirmEntry: false,
  announceTemplate:
    "🎉 Congratulations ${winner}! You won ${prize}! (${entries} entered)",
  enabled: true,
});

function formatWhen(iso: string) {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export function Giveaways() {
  const [items, setItems] = useState<Giveaway[]>([]);
  const [active, setActive] = useState<ActiveGiveaway | null>(null);
  const [history, setHistory] = useState<GiveawayRunHistory[]>([]);
  const [edit, setEdit] = useState<Giveaway | null>(null);
  const [toast, setToast] = useState("");

  async function load() {
    const [gws, act, hist] = await Promise.all([
      api.listGiveaways(),
      api.getActiveGiveaway(),
      api.listGiveawayHistory(50),
    ]);
    setItems(gws);
    setActive(act);
    setHistory(hist);
  }

  useEffect(() => {
    load();
    const u = listen("giveaway-updated", () => load());
    return () => {
      u.then((fn) => fn());
    };
  }, []);

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Giveaways</h1>
          <p>
            Link an entry command, then draw fairly with a mod command or a
            countdown. Winners are picked with true OS randomness and kept in
            history below.
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setEdit(blank())}>
          New giveaway
        </button>
      </div>

      {active && (
        <div className="panel" style={{ marginBottom: 16 }}>
          <h3 style={{ marginTop: 0 }}>Running now</h3>
          <p>
            <strong>{active.giveaway.title}</strong> · {active.entryCount}{" "}
            entered
            {active.endsAt
              ? ` · ends ${new Date(active.endsAt).toLocaleTimeString()}`
              : ""}
          </p>
          <div className="btn-row">
            <button
              className="btn btn-accent"
              onClick={async () => {
                const w = await api.drawGiveaway();
                setToast(
                  w.length
                    ? `Winner: ${w.map((x) => x.login).join(", ")}`
                    : "No winners",
                );
                load();
              }}
            >
              Pick winner now
            </button>
            <button
              className="btn btn-ghost"
              onClick={async () => {
                await api.stopGiveaway();
                load();
              }}
            >
              Stop
            </button>
          </div>
        </div>
      )}

      <div className="panel" style={{ marginBottom: 16 }}>
        <h3 style={{ marginTop: 0 }}>Winner history</h3>
        {history.length === 0 ? (
          <div className="empty" style={{ padding: 16 }}>
            No winners yet. Draw a giveaway to start tracking them here.
          </div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>When</th>
                <th>Giveaway</th>
                <th>Entries</th>
                <th>Winners</th>
              </tr>
            </thead>
            <tbody>
              {history.map((h) => (
                <tr key={h.runId}>
                  <td>{formatWhen(h.startedAt)}</td>
                  <td>
                    <strong>{h.title}</strong>
                    <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                      {h.prize || "No prize label"}
                    </div>
                  </td>
                  <td>{h.entryCount}</td>
                  <td>
                    {h.winners.map((w) => (
                      <span
                        key={w.userId}
                        className="badge on"
                        style={{ marginRight: 6 }}
                      >
                        @{w.login}
                      </span>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">Create a giveaway to get started.</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Title</th>
                <th>Enter</th>
                <th>Draw</th>
                <th>Duration</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {items.map((g) => (
                <tr key={g.id}>
                  <td>
                    <strong>{g.title}</strong>
                    <div style={{ color: "var(--muted)", fontSize: "0.85rem" }}>
                      {g.prize || "No prize label"}
                    </div>
                  </td>
                  <td>
                    <code>{g.entryCommand}</code>
                  </td>
                  <td>
                    <code>{g.drawCommand}</code>
                  </td>
                  <td>{g.durationMins ? `${g.durationMins} min` : "Manual"}</td>
                  <td>
                    <div className="btn-row">
                      <button
                        className="btn btn-accent"
                        onClick={async () => {
                          await api.startGiveaway(g.id);
                          load();
                        }}
                      >
                        Start
                      </button>
                      <button className="btn btn-ghost" onClick={() => setEdit(g)}>
                        Edit
                      </button>
                      <button
                        className="btn btn-danger"
                        onClick={async () => {
                          await api.deleteGiveaway(g.id);
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
            <h2>{edit.id ? "Edit giveaway" : "New giveaway"}</h2>
            <div className="field">
              <label>Title</label>
              <input
                value={edit.title}
                onChange={(e) => setEdit({ ...edit, title: e.target.value })}
              />
            </div>
            <div className="field">
              <label>Prize</label>
              <input
                value={edit.prize}
                onChange={(e) => setEdit({ ...edit, prize: e.target.value })}
                placeholder="Steam key, merch, etc."
              />
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Entry command</label>
                <input
                  value={edit.entryCommand}
                  onChange={(e) =>
                    setEdit({ ...edit, entryCommand: e.target.value })
                  }
                />
              </div>
              <div className="field">
                <label>Draw command (mods)</label>
                <input
                  value={edit.drawCommand}
                  onChange={(e) =>
                    setEdit({ ...edit, drawCommand: e.target.value })
                  }
                />
              </div>
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Auto-draw after (minutes, blank = manual)</label>
                <input
                  type="number"
                  value={edit.durationMins ?? ""}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      durationMins: e.target.value
                        ? Number(e.target.value)
                        : null,
                    })
                  }
                />
              </div>
              <div className="field">
                <label>Number of winners</label>
                <input
                  type="number"
                  value={edit.winnerCount}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      winnerCount: Number(e.target.value) || 1,
                    })
                  }
                />
              </div>
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Eligibility</label>
                <select
                  value={edit.eligibility}
                  onChange={(e) =>
                    setEdit({ ...edit, eligibility: e.target.value })
                  }
                >
                  <option value="everyone">Everyone</option>
                  <option value="sub">Subscribers</option>
                </select>
              </div>
              <div className="field">
                <label>Confirm entries in chat</label>
                <select
                  value={edit.confirmEntry ? "1" : "0"}
                  onChange={(e) =>
                    setEdit({ ...edit, confirmEntry: e.target.value === "1" })
                  }
                >
                  <option value="0">Silent</option>
                  <option value="1">Confirm</option>
                </select>
              </div>
            </div>
            <div className="field">
              <label>
                Winner message — ${"{winner}"}, ${"{prize}"}, ${"{entries}"}
              </label>
              <textarea
                value={edit.announceTemplate}
                onChange={(e) =>
                  setEdit({ ...edit, announceTemplate: e.target.value })
                }
              />
            </div>
            <label style={{ display: "flex", gap: 8, marginBottom: 14 }}>
              <input
                type="checkbox"
                checked={edit.excludeMods}
                onChange={(e) =>
                  setEdit({ ...edit, excludeMods: e.target.checked })
                }
              />
              Exclude mods from entering
            </label>
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertGiveaway(edit);
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

      {toast && (
        <div className="toast" onClick={() => setToast("")}>
          {toast}
        </div>
      )}
    </>
  );
}
