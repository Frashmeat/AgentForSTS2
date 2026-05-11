// 顶层布局：侧边导航 + 内容区。
// 用 HashRouter 兼容 Tauri webview / 静态 web 部署两种环境。

import { NavLink, Outlet } from "react-router-dom";

const NAV_LINKS = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/editor", label: "Mod Editor" },
  { to: "/batch", label: "Batch Generate" },
  { to: "/log", label: "Log Analysis" },
];

export function Layout() {
  return (
    <div className="min-h-screen flex">
      <nav className="w-56 border-r border-muted/30 p-4 space-y-1 shrink-0">
        <p className="text-sm font-semibold mb-3">AgentTheSpire</p>
        <p className="text-xs text-muted mb-4">
          {__IS_TAURI__ ? "Tauri desktop" : "Web browser"} · v{__APP_VERSION__}
        </p>
        {NAV_LINKS.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.end}
            className={({ isActive }) =>
              `block px-3 py-1.5 rounded text-sm ${
                isActive
                  ? "bg-accent/10 text-accent font-medium"
                  : "text-muted hover:bg-muted/10 hover:text-foreground"
              }`
            }
          >
            {link.label}
          </NavLink>
        ))}
      </nav>
      <main className="flex-1 p-8 max-w-4xl">
        <Outlet />
      </main>
    </div>
  );
}
