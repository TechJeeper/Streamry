import { useEffect, useRef, useState, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { useOpenFromQuery } from "../hooks/useOpenFromQuery";
import type { MediaClip, OverlayInfo } from "../types";

const GRID_W = 16;
const GRID_H = 9;

const DEFAULT_RECT = { overlayX: 4, overlayY: 2, overlayW: 8, overlayH: 5 };

function withDefaults(clip: MediaClip): MediaClip {
  return {
    ...clip,
    overlayX: clip.overlayX ?? DEFAULT_RECT.overlayX,
    overlayY: clip.overlayY ?? DEFAULT_RECT.overlayY,
    overlayW: clip.overlayW ?? DEFAULT_RECT.overlayW,
    overlayH: clip.overlayH ?? DEFAULT_RECT.overlayH,
    chromaKey: clip.chromaKey ?? "",
    chromaTolerance: clip.chromaTolerance ?? 64,
  };
}

function rectLabel(clip: MediaClip) {
  const w = clip.overlayW ?? DEFAULT_RECT.overlayW;
  const h = clip.overlayH ?? DEFAULT_RECT.overlayH;
  return `${w}×${h}`;
}

function needsPlacement(type: string) {
  return type === "image" || type === "gif" || type === "video";
}

function needsChroma(type: string) {
  return type === "image" || type === "gif" || type === "video";
}

function needsDuration(type: string) {
  return type === "image" || type === "gif" || type === "video" || type === "sound";
}

function needsVolume(type: string) {
  return type === "sound" || type === "video";
}

type Cell = { c: number; r: number };
type Rect = { overlayX: number; overlayY: number; overlayW: number; overlayH: number };

function rectFromCells(a: Cell, b: Cell): Rect {
  return {
    overlayX: Math.min(a.c, b.c),
    overlayY: Math.min(a.r, b.r),
    overlayW: Math.abs(a.c - b.c) + 1,
    overlayH: Math.abs(a.r - b.r) + 1,
  };
}

function cellFromPointer(e: React.PointerEvent, el: HTMLElement): Cell {
  const box = el.getBoundingClientRect();
  const c = Math.floor(((e.clientX - box.left) / box.width) * GRID_W);
  const r = Math.floor(((e.clientY - box.top) / box.height) * GRID_H);
  return {
    c: Math.max(0, Math.min(GRID_W - 1, c)),
    r: Math.max(0, Math.min(GRID_H - 1, r)),
  };
}

function PlacementGrid({
  x,
  y,
  w,
  h,
  onChange,
}: {
  x: number;
  y: number;
  w: number;
  h: number;
  onChange: (rect: Rect) => void;
}) {
  const gridRef = useRef<HTMLDivElement>(null);
  const dragStart = useRef<Cell | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const [live, setLive] = useState<Rect | null>(null);

  const shown = live ?? { overlayX: x, overlayY: y, overlayW: w, overlayH: h };

  function inSelection(c: number, r: number) {
    return (
      c >= shown.overlayX &&
      c < shown.overlayX + shown.overlayW &&
      r >= shown.overlayY &&
      r < shown.overlayY + shown.overlayH
    );
  }

  return (
    <div
      ref={gridRef}
      className="place-grid"
      role="group"
      aria-label="Overlay placement grid"
      onPointerDown={(e) => {
        if (!gridRef.current) return;
        e.preventDefault();
        const cell = cellFromPointer(e, gridRef.current);
        dragStart.current = cell;
        setLive(rectFromCells(cell, cell));
        gridRef.current.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (!dragStart.current || !gridRef.current) return;
        const cell = cellFromPointer(e, gridRef.current);
        setLive(rectFromCells(dragStart.current, cell));
      }}
      onPointerUp={(e) => {
        if (!dragStart.current || !gridRef.current) return;
        const cell = cellFromPointer(e, gridRef.current);
        const rect = rectFromCells(dragStart.current, cell);
        dragStart.current = null;
        setLive(null);
        onChangeRef.current(rect);
        try {
          gridRef.current.releasePointerCapture(e.pointerId);
        } catch {
          /* already released */
        }
      }}
    >
      {Array.from({ length: GRID_H * GRID_W }, (_, i) => {
        const c = i % GRID_W;
        const r = Math.floor(i / GRID_W);
        return (
          <div
            key={`${c}-${r}`}
            className={`place-cell${inSelection(c, r) ? " on" : ""}`}
            aria-hidden
          />
        );
      })}
    </div>
  );
}

function ChromaKeyControls({
  enabled,
  color,
  tolerance,
  previewUrl,
  mediaType,
  onChange,
}: {
  enabled: boolean;
  color: string;
  tolerance: number;
  previewUrl: string;
  mediaType: string;
  onChange: (next: {
    chromaKey: string;
    chromaTolerance: number;
  }) => void;
}) {
  const [eyeErr, setEyeErr] = useState("");
  const keyColor = color && /^#[0-9a-fA-F]{6}$/.test(color) ? color : "#00FF00";

  async function pickEyedropper() {
    setEyeErr("");
    const ED = (
      window as unknown as {
        EyeDropper?: new () => { open: () => Promise<{ sRGBHex: string }> };
      }
    ).EyeDropper;
    if (!ED) {
      setEyeErr("Eyedropper isn’t available in this window — use the color picker.");
      return;
    }
    try {
      const result = await new ED().open();
      const hex = result.sRGBHex.toUpperCase();
      onChange({ chromaKey: hex, chromaTolerance: tolerance });
    } catch {
      /* user cancelled */
    }
  }

  return (
    <div className="field">
      <label>Chromakey</label>
      <label className="chroma-enable">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) =>
            onChange({
              chromaKey: e.target.checked ? keyColor : "",
              chromaTolerance: tolerance,
            })
          }
        />
        Make a color transparent on the overlay
      </label>
      {enabled && (
        <>
          <div className="chroma-row">
            <input
              type="color"
              className="chroma-swatch"
              value={keyColor}
              onChange={(e) =>
                onChange({
                  chromaKey: e.target.value.toUpperCase(),
                  chromaTolerance: tolerance,
                })
              }
              aria-label="Chromakey color"
            />
            <code className="chroma-hex">{keyColor}</code>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={pickEyedropper}
              title="Pick a color from the screen"
            >
              Eyedropper
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() =>
                onChange({ chromaKey: "", chromaTolerance: tolerance })
              }
            >
              Clear
            </button>
          </div>
          <div className="chroma-tol">
            <label htmlFor="chroma-tol">Tolerance: {tolerance}</label>
            <input
              id="chroma-tol"
              type="range"
              min={0}
              max={120}
              value={tolerance}
              onChange={(e) =>
                onChange({
                  chromaKey: keyColor,
                  chromaTolerance: Number(e.target.value) || 0,
                })
              }
            />
          </div>
          {previewUrl && (
            <div className="chroma-preview-wrap">
              <p className="field-hint">
                Preview — use the eyedropper on this image (or anywhere on
                screen) to sample the key color. Raise tolerance if fringe
                remains.
              </p>
              {mediaType === "video" ? (
                <video
                  className="chroma-preview"
                  src={previewUrl}
                  muted
                  playsInline
                  controls
                />
              ) : (
                <img className="chroma-preview" src={previewUrl} alt="" />
              )}
            </div>
          )}
          {eyeErr && <p className="field-hint" style={{ color: "var(--danger)" }}>{eyeErr}</p>}
        </>
      )}
    </div>
  );
}

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

  const openEdit = useCallback((m: MediaClip) => setEdit(withDefaults(m)), []);
  useOpenFromQuery(items, openEdit);

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
      const clip = await api.importMedia(file);
      await load();
      setEdit(withDefaults(clip));
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
          <div className="media-list">
            {items.map((m) => (
              <div key={m.id} className="media-row">
                <div className="media-row-main">
                  <strong className="media-row-name">{m.name}</strong>
                  <div className="media-row-meta">
                    <span className="badge">{m.mediaType}</span>
                    {needsDuration(m.mediaType) &&
                      !(m.mediaType === "image" && m.alwaysShow) && (
                        <span>{(m.durationMs / 1000).toFixed(1)}s</span>
                      )}
                    {m.mediaType === "image" && m.alwaysShow && (
                      <span>Always show</span>
                    )}
                    {needsVolume(m.mediaType) && <span>{m.volume}%</span>}
                    {needsPlacement(m.mediaType) && (
                      <span>{rectLabel(withDefaults(m))} region</span>
                    )}
                    {needsChroma(m.mediaType) && m.chromaKey && (
                      <span className="chroma-badge" title={m.chromaKey}>
                        <span
                          className="chroma-dot"
                          style={{ background: m.chromaKey }}
                        />
                        Key
                      </span>
                    )}
                  </div>
                </div>
                <div className="btn-row media-row-actions">
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
                    onClick={() => setEdit(withDefaults(m))}
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
              </div>
            ))}
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

            {edit.mediaType === "image" && (
              <label
                style={{
                  display: "flex",
                  gap: 10,
                  marginBottom: 14,
                  alignItems: "center",
                }}
              >
                <input
                  type="checkbox"
                  checked={!!edit.alwaysShow}
                  onChange={(e) =>
                    setEdit({ ...edit, alwaysShow: e.target.checked })
                  }
                />
                Always show on overlay (no duration — for logos / frames)
              </label>
            )}

            {(needsDuration(edit.mediaType) || needsVolume(edit.mediaType)) && (
              <div className="grid-2">
                {needsDuration(edit.mediaType) &&
                  !(edit.mediaType === "image" && edit.alwaysShow) && (
                    <div className="field">
                      <label>
                        {edit.mediaType === "image" || edit.mediaType === "gif"
                          ? "Display duration (seconds)"
                          : edit.mediaType === "video"
                            ? "Max duration (seconds)"
                            : "Duration (seconds)"}
                      </label>
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
                      {(edit.mediaType === "image" ||
                        edit.mediaType === "gif") && (
                        <p className="field-hint">
                          How long the image stays on the OBS overlay.
                        </p>
                      )}
                    </div>
                  )}
                {needsVolume(edit.mediaType) && (
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
                )}
              </div>
            )}

            {needsPlacement(edit.mediaType) && (
              <div className="field">
                <label>Overlay placement</label>
                <PlacementGrid
                  x={edit.overlayX ?? DEFAULT_RECT.overlayX}
                  y={edit.overlayY ?? DEFAULT_RECT.overlayY}
                  w={edit.overlayW ?? DEFAULT_RECT.overlayW}
                  h={edit.overlayH ?? DEFAULT_RECT.overlayH}
                  onChange={(rect) => setEdit({ ...edit, ...rect })}
                />
                <p className="field-hint">
                  Drag across the 16×9 grid to set position and size. Selected:{" "}
                  <strong>
                    {edit.overlayW ?? DEFAULT_RECT.overlayW}×
                    {edit.overlayH ?? DEFAULT_RECT.overlayH}
                  </strong>{" "}
                  cells.
                </p>
              </div>
            )}

            {needsChroma(edit.mediaType) && (
              <ChromaKeyControls
                enabled={!!edit.chromaKey}
                color={edit.chromaKey || "#00FF00"}
                tolerance={edit.chromaTolerance ?? 64}
                previewUrl={
                  overlay?.url
                    ? `${overlay.url.replace(/\/$/, "")}/media/${edit.fileName}`
                    : ""
                }
                mediaType={edit.mediaType}
                onChange={(next) => setEdit({ ...edit, ...next })}
              />
            )}

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
                  const next = withDefaults(edit);
                  await api.upsertMedia({
                    ...next,
                    alwaysShow:
                      next.mediaType === "image" ? !!next.alwaysShow : false,
                    chromaKey: needsChroma(next.mediaType)
                      ? next.chromaKey || ""
                      : "",
                    chromaTolerance: next.chromaTolerance ?? 64,
                  });
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
    api
      .getOverlayInfo()
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
