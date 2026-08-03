import { useCallback, useEffect, useState } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { api } from "./api";
import { Shell } from "./components/Shell";
import { Automations } from "./pages/Automations";
import { Commands } from "./pages/Commands";
import { Dashboard } from "./pages/Dashboard";
import { Giveaways } from "./pages/Giveaways";
import { Media } from "./pages/Media";
import { Settings } from "./pages/Settings";
import { Setup } from "./pages/Setup";
import { Timers } from "./pages/Timers";
import { Variables } from "./pages/Variables";
import { applyTheme, readCachedTheme } from "./theme";
import type { RuntimeStatus } from "./types";
import "./styles.css";

applyTheme(readCachedTheme());

function App() {
  const [ready, setReady] = useState(false);
  const [setupComplete, setSetupComplete] = useState(false);
  const [status, setStatus] = useState<RuntimeStatus | null>(null);

  const refreshStatus = useCallback(() => {
    api.getStatus().then(setStatus).catch(() => {});
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [s, settings] = await Promise.all([
          api.getStatus(),
          api.getSettings().catch(() => null),
        ]);
        setStatus(s);
        setSetupComplete(s.setupComplete);
        if (settings?.theme) applyTheme(settings.theme);
      } catch {
        setSetupComplete(false);
      } finally {
        setReady(true);
      }
    })();
  }, []);

  useEffect(() => {
    if (!ready || !setupComplete) return;
    const unsub = import("@tauri-apps/api/event").then(({ listen }) =>
      listen("status-changed", () => refreshStatus()),
    );
    return () => {
      unsub.then((u) => u());
    };
  }, [ready, setupComplete, refreshStatus]);

  if (!ready) {
    return (
      <div className="setup">
        <div className="setup-card">
          <h1>Streamry</h1>
          <p className="lead">Loading…</p>
        </div>
      </div>
    );
  }

  if (!setupComplete) {
    return (
      <Setup
        onDone={() => {
          setSetupComplete(true);
          refreshStatus();
        }}
      />
    );
  }

  return (
    <BrowserRouter>
      <Shell status={status}>
        <Routes>
          <Route
            path="/"
            element={
              <Dashboard status={status} refreshStatus={refreshStatus} />
            }
          />
          <Route path="/commands" element={<Commands />} />
          <Route path="/timers" element={<Timers />} />
          <Route path="/giveaways" element={<Giveaways />} />
          <Route path="/automations" element={<Automations />} />
          <Route path="/media" element={<Media />} />
          <Route path="/variables" element={<Variables />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="/import" element={<Navigate to="/settings" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Shell>
    </BrowserRouter>
  );
}

export default App;
