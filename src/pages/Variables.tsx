import { useEffect, useState } from "react";
import { api } from "../api";
import type { CustomVariable } from "../types";

const blank = (): CustomVariable => ({
  id: "",
  name: "",
  value: "",
});

export function Variables() {
  const [items, setItems] = useState<CustomVariable[]>([]);
  const [edit, setEdit] = useState<CustomVariable | null>(null);

  async function load() {
    setItems(await api.listVariables());
  }
  useEffect(() => {
    load();
  }, []);

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Variables</h1>
          <p>
            Named values for commands, timers, and automations — use{" "}
            <code>${"{name}"}</code> in messages.
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setEdit(blank())}>
          New variable
        </button>
      </div>

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">
            No variables yet. Add one or import from StreamElements in Settings.
          </div>
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th className="col-short">Name</th>
                  <th>Value</th>
                  <th className="col-actions"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((v) => (
                  <tr key={v.id}>
                    <td>
                      <strong>${`{${v.name}}`}</strong>
                    </td>
                    <td>{v.value}</td>
                    <td className="col-actions">
                      <div className="btn-row">
                        <button className="btn btn-ghost" onClick={() => setEdit(v)}>
                          Edit
                        </button>
                        <button
                          className="btn btn-danger"
                          onClick={async () => {
                            await api.deleteVariable(v.id);
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
            <h2>{edit.id ? "Edit variable" : "New variable"}</h2>
            <div className="field">
              <label>Name (letters used inside ${"{…}"})</label>
              <input
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
                placeholder="e.g. discord"
              />
            </div>
            <div className="field">
              <label>Value</label>
              <textarea
                value={edit.value}
                onChange={(e) => setEdit({ ...edit, value: e.target.value })}
                placeholder="Text inserted wherever the variable is used"
              />
            </div>
            <p className="hint">
              Example: set name <code>discord</code>, then use{" "}
              <code>${"{discord}"}</code> in a command response.
            </p>
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertVariable(edit);
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
