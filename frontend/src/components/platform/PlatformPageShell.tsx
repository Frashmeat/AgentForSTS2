import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

export function PlatformPageShell({
  kicker,
  title,
  description,
  actions,
  width = "wide",
  children,
}: {
  kicker: string;
  title: ReactNode;
  description: ReactNode;
  actions?: ReactNode;
  width?: "narrow" | "wide";
  children: ReactNode;
}) {
  return (
    <div className="platform-page-shell">
      <div className={cn("platform-page-frame", width === "narrow" ? "platform-page-frame-narrow" : "platform-page-frame-wide")}>
        <header className="platform-page-hero">
          <div className="platform-page-hero-copy">
            <p className="platform-page-kicker">{kicker}</p>
            <h1>{title}</h1>
            <div className="platform-page-description">{description}</div>
          </div>
          {actions ? <div className="platform-page-hero-actions">{actions}</div> : null}
        </header>

        <main className="platform-page-body">{children}</main>
      </div>
    </div>
  );
}

export default PlatformPageShell;
