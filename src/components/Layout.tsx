import { useEffect, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";

const LINKS = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/editor", label: "Mod" },
  { to: "/composition", label: "Composition" },
  { to: "/batch", label: "Batch" },
  { to: "/log", label: "Log" },
  { to: "/runs", label: "Runs" },
  { to: "/system", label: "System" },
];
const THEMES = ["vermilion", "indigo", "jade", "ink"] as const;
type Theme = (typeof THEMES)[number];

export function Layout() {
  const [theme, setTheme] = useState<Theme>(() => {
    const value = localStorage.getItem("agentthespire-theme-v1");
    return THEMES.includes(value as Theme) ? (value as Theme) : "vermilion";
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("agentthespire-theme-v1", theme);
  }, [theme]);

  return (
    <div className="min-h-screen flex flex-col">
      <header className="sticky top-0 z-40 bg-paper-soft" style={{ borderBottom: "1px solid var(--rule)" }}>
        <div className="max-w-[1480px] mx-auto px-7 py-3 flex items-center gap-5 flex-wrap">
          <div className="brand-logo" aria-hidden="true">塔</div>
          <div className="flex-1 min-w-0">
            <div className="eyebrow">AgentTheSpire · Stage 2</div>
            <div className="font-display text-[22px] text-ink">STS2 Mod Workstation</div>
          </div>
          <div className="theme-switch" role="group" aria-label="主题">
            {THEMES.map((value) => (
              <button
                key={value}
                type="button"
                data-theme={value}
                className={`theme-chip ${theme === value ? "active" : ""}`}
                aria-label={value}
                onClick={() => setTheme(value)}
              >
                <span className="swatch" />
              </button>
            ))}
          </div>
        </div>
        <nav className="max-w-[1480px] mx-auto px-7 pb-3 flex gap-2 flex-wrap">
          {LINKS.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.end}
              className={({ isActive }) => `topbtn ${isActive ? "active" : "ghost"}`}
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className="flex-1 w-full max-w-[1480px] mx-auto px-7 pt-8 pb-14">
        <Outlet />
      </main>
    </div>
  );
}
