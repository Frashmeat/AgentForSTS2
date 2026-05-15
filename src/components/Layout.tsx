// 顶层布局：墨青基调顶栏（brand-logo + 路由 nav + 主题切换） + 内容区 + footnote。
// 视觉沿用 e—flowcode 三件套（image-web / video-web / diagram-web）模板。
//
// 主题 vermilion / indigo / jade / ink，本地 localStorage 记忆。
// 改 data-theme 走 <html>，CSS 变量层级生效。

import { useEffect, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";

const NAV_LINKS = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/editor", label: "Mod Editor" },
  { to: "/batch", label: "Batch" },
  { to: "/log", label: "Log Analysis" },
  { to: "/settings", label: "Settings" },
];

const THEMES = ["vermilion", "indigo", "jade", "ink"] as const;
type Theme = (typeof THEMES)[number];
const THEME_KEY = "agentthespire-theme-v1";
const DEFAULT_THEME: Theme = "vermilion";
const THEME_LABEL: Record<Theme, string> = {
  vermilion: "朱砂",
  indigo: "靛青",
  jade: "松绿",
  ink: "墨黑",
};

function readInitialTheme(): Theme {
  if (typeof window === "undefined") return DEFAULT_THEME;
  try {
    const t = localStorage.getItem(THEME_KEY);
    if (t && (THEMES as readonly string[]).includes(t)) return t as Theme;
  } catch {
    // 部分 webview 拿不到 storage，回退默认
  }
  return DEFAULT_THEME;
}

export function Layout() {
  const [theme, setTheme] = useState<Theme>(readInitialTheme);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      // ignore
    }
  }, [theme]);

  return (
    <div className="min-h-screen flex flex-col">
      <header
        className="sticky top-0 z-40 bg-paper-soft"
        style={{ borderBottom: "1.5px solid var(--rule)" }}
      >
        <div
          className="max-w-[1480px] mx-auto px-7 pt-3.5 pb-2.5 flex items-center gap-6 flex-wrap"
        >
          {/* 品牌区 */}
          <div className="flex items-center gap-3.5 flex-1 min-w-0">
            <div className="brand-logo" aria-hidden="true">塔</div>
            <div className="flex flex-col leading-[1.05] gap-0.5 min-w-0">
              <span className="eyebrow">AgentTheSpire · est. 2026</span>
              <span
                className="font-display text-[24px] font-medium text-ink mt-0.5"
                style={{ letterSpacing: "-0.025em" }}
              >
                AgentTheSpire
                <em
                  className="not-italic font-display ml-1"
                  style={{
                    fontStyle: "italic",
                    fontVariationSettings: '"opsz" 144, "SOFT" 60',
                    fontWeight: 400,
                    color: "var(--accent)",
                  }}
                >
                  for STS2
                </em>
              </span>
              <span
                className="font-mono text-[10px] uppercase mt-px"
                style={{ letterSpacing: "0.14em", color: "var(--ink-mute)" }}
              >
                {__IS_TAURI__ ? "tauri desktop" : "web browser"} · v
                {__APP_VERSION__} · slay-the-spire 2 mod cockpit
              </span>
            </div>
          </div>

          {/* 顶栏右侧：主题切换 */}
          <div className="flex items-center gap-2">
            <div
              className="theme-switch"
              role="group"
              aria-label="主题切换"
            >
              {THEMES.map((t) => (
                <button
                  key={t}
                  type="button"
                  data-theme={t}
                  className={`theme-chip ${theme === t ? "active" : ""}`}
                  aria-label={THEME_LABEL[t]}
                  onClick={() => setTheme(t)}
                >
                  <span className="swatch" />
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* 路由 nav 条 */}
        <nav
          className="max-w-[1480px] mx-auto px-7 pb-3 flex gap-2 flex-wrap"
          style={{ borderTop: "1px solid var(--rule-soft)", paddingTop: "10px" }}
        >
          {NAV_LINKS.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.end}
              className={({ isActive }) =>
                `topbtn ${isActive ? "active" : "ghost"}`
              }
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
      </header>

      <main
        className="flex-1 w-full max-w-[1480px] mx-auto px-7 pt-8 pb-14"
        style={{ position: "relative", zIndex: 1 }}
      >
        <Outlet />
      </main>

      <footer
        className="max-w-[1480px] mx-auto w-full px-7 pt-4 pb-7 flex justify-between items-center gap-6 font-mono"
        style={{
          fontWeight: 500,
          fontSize: "10.5px",
          letterSpacing: "0.16em",
          textTransform: "uppercase",
          color: "var(--ink-faint)",
          borderTop: "1px solid var(--rule-hair)",
          marginTop: "28px",
        }}
      >
        <span>agentthespire / sts2 · vol. iii · stage 3</span>
        <span>★ rust · tauri · react</span>
      </footer>
    </div>
  );
}
