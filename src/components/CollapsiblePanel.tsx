import type { ReactNode } from "react";

export function CollapsiblePanel({
  title,
  blurb,
  children,
  defaultOpen = false,
}: {
  title: string;
  blurb?: string;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  return (
    <details className="panel collapse-panel" open={defaultOpen || undefined}>
      <summary className="collapse-summary">
        <span className="collapse-title">{title}</span>
        {blurb ? <span className="collapse-blurb">{blurb}</span> : null}
      </summary>
      <div className="collapse-body">{children}</div>
    </details>
  );
}
