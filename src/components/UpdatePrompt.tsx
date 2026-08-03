import { openUrl } from "@tauri-apps/plugin-opener";
import type { UpdateCheck } from "../types";

export function UpdatePrompt({
  update,
  onDismiss,
}: {
  update: UpdateCheck;
  onDismiss: () => void;
}) {
  return (
    <div className="modal-backdrop">
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Update available</h2>
        <p style={{ color: "var(--muted)", marginTop: 0 }}>
          Streamry <strong>{update.latestVersion}</strong> is available. You’re
          on {update.currentVersion}.
        </p>
        {update.notes && (
          <p style={{ color: "var(--muted)", marginTop: 0 }}>{update.notes}</p>
        )}
        <div className="btn-row">
          <button className="btn btn-ghost" onClick={onDismiss}>
            Not now
          </button>
          <button
            className="btn btn-primary"
            onClick={async () => {
              await openUrl(update.downloadUrl);
              onDismiss();
            }}
          >
            Go to downloads
          </button>
        </div>
      </div>
    </div>
  );
}
