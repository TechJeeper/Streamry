import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { MediaClip, OverlayInfo } from "../types";

export function Media() {
  const [items, setItems] = useState<MediaClip[]>([]);
  const [overlay, setOverlay] = useState<OverlayInfo | null>(null);
  const [edit, setEdit] = useState<MediaClip | null>(null);
  const [copied, setCopied] = useState(false);
  const [err, setErr] = useState("");

  async function load() {
    const [list, info] = await Promise.all([
      api.listMedia(),
      api.getOverlayInfo(),
    ]);
    setItems(list);
    setOverlay(info);
  }

  useEffect(() => {
    load().catch((e) => setErr(String(e)));
  }, []);

  async function copyUrl() {
    if (!overlay?.url) return;
    try {
      await navigator.clipboard.writeText(overlay.url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      setErr("Could not copy URL");
    }
  }

  async function addFile() {
    setErr("");
    const file = await open({
      multiple: false,
      filters: [
        {
          name: "Media",
          extensions: [
            "mp3",
            "wav",
            "ogg",
            "m4a",
            "gif",
            "png",
            "jpg",
            "jpeg",
            "webp",
            "mp4",
            "webm",
            "mov",
          ],
        },
      ],
    });
    if (!file || Array.isArray(file)) return;
    try {
      await api.importMedia(file);
      await load();
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Media</h1>
          <p>
            Sounds, GIFs, images, and video clips for commands &amp; automations
            — played in OBS via a browser source.
          </p>
        </div>
        <button className="btn btn-primary" onClick={addFile}>
          Add media file
        </button>
      </div>

      {err && <p style={{ color: "var(--danger)" }}>{err}</p>}

      <div className="panel obs-callout" style={{ marginBottom: 16 }}>
        <div>
          <div className="label">OBS browser source</div>
          <code className="obs-url">{overlay?.url ?? "…"}</code>
          <p className="obs-hint">
            In OBS: Add → Browser → paste this URL. Set width/height to your
            canvas (e.g. 1920×1080), check{" "}
            <em>Control audio via OBS</em> if you want, and leave the page
            transparent.
          </p>
        </div>
        <button className="btn btn-accent" onClick={copyUrl} disabled={!overlay}>
          {copied ? "Copied" : "Copy URL"}
        </button>
      </div>

      <div className="panel">
        {items.length === 0 ? (
          <div className="empty">
            No media yet. Add a sound, GIF, image, or video clip, then attach it
            to a command or automation.
          </div>
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th className="col-short">Type</th>
                  <th className="col-short">Duration</th>
                  <th className="col-short">Volume</th>
                  <th className="col-actions"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((m) => (
                  <tr key={m.id}>
                    <td>
                      <strong>{m.name}</strong>
                    </td>
                    <td>
                      <span className="badge">{m.mediaType}</span>
                    </td>
                    <td>{(m.durationMs / 1000).toFixed(1)}s</td>
                    <td>{m.volume}%</td>
                    <td className="col-actions">
                      <div className="btn-row">
                        <button
                          className="btn btn-ghost"
                          onClick={async () => {
                            try {
                              await api.testMedia(m.id);
                            } catch (e) {
                              setErr(String(e));
                            }
                          }}
                        >
                          Test
                        </button>
                        <button
                          className="btn btn-ghost"
                          onClick={() => setEdit(m)}
                        >
                          Edit
                        </button>
                        <button
                          className="btn btn-danger"
                          onClick={async () => {
                            await api.deleteMedia(m.id);
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
            <h2>Edit media</h2>
            <div className="field">
              <label>Name</label>
              <input
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
              />
            </div>
            <div className="grid-2">
              <div className="field">
                <label>Duration (seconds)</label>
                <input
                  type="number"
                  min={0.5}
                  step={0.5}
                  value={edit.durationMs / 1000}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      durationMs: Math.round(
                        (Number(e.target.value) || 0.5) * 1000,
                      ),
                    })
                  }
                />
              </div>
              <div className="field">
                <label>Volume (%)</label>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={edit.volume}
                  onChange={(e) =>
                    setEdit({
                      ...edit,
                      volume: Math.max(
                        0,
                        Math.min(100, Number(e.target.value) || 0),
                      ),
                    })
                  }
                />
              </div>
            </div>
            <p style={{ color: "var(--muted)", fontSize: "0.9rem" }}>
              Type: {edit.mediaType} · file is managed by Streamry
            </p>
            <div className="btn-row">
              <button className="btn btn-ghost" onClick={() => setEdit(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  await api.upsertMedia(edit);
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

/** Compact OBS URL banner for Commands / Automations pages */
export function ObsBrowserSourceBanner() {
  const [url, setUrl] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    api.getOverlayInfo()
      .then((i) => setUrl(i.url))
      .catch(() => {});
  }, []);

  if (!url) return null;

  return (
    <div className="obs-inline">
      <span>
        OBS browser source: <code>{url}</code>
      </span>
      <button
        type="button"
        className="btn btn-ghost"
        style={{ padding: "6px 10px", fontSize: "0.85rem" }}
        onClick={async () => {
          await navigator.clipboard.writeText(url);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
