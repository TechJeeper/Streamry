import { NavLink } from "react-router-dom";
import type { RuntimeStatus } from "../types";

const links = [
  ["/", "Dashboard"],
  ["/commands", "Commands"],
  ["/timers", "Timers"],
  ["/giveaways", "Giveaways"],
  ["/automations", "Automations"],
  ["/media", "Media"],
  ["/variables", "Variables"],
  ["/settings", "Settings"],
];

export function Shell({
  status,
  children,
}: {
  status: RuntimeStatus | null;
  children: React.ReactNode;
}) {
  const connected = status?.connected;
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src="/sentinel.svg" alt="" />
          <div className="brand-text">
            <div className="brand-name">Streamry</div>
            <div className="brand-tag">Local Twitch bot</div>
          </div>
        </div>
        <nav className="nav">
          {links.map(([to, label]) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) => (isActive ? "active" : undefined)}
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className={`status-dot ${connected ? "on" : ""}`} />
          {connected
            ? `Online · #${status?.channel ?? "—"}`
            : status?.connecting
              ? "Connecting…"
              : "Offline"}
          {status?.live ? " · LIVE" : ""}
        </div>
      </aside>
      <main className="main">{children}</main>
    </div>
  );
}
