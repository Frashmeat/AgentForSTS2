import {
  Activity,
  ClipboardList,
  Gauge,
  HeartPulse,
  Home,
  KeyRound,
  Library,
  ReceiptText,
  ServerCog,
  UsersRound,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";

type AdminNavItem = {
  label: string;
  path: string;
  icon: LucideIcon;
};

const adminNavGroups: Array<{ label: string; items: AdminNavItem[] }> = [
  {
    label: "总览",
    items: [
      { label: "管理台首页", path: "/admin", icon: Home },
      { label: "运行状态", path: "/admin/runtime", icon: Gauge },
      { label: "执行记录", path: "/admin/executions", icon: ClipboardList },
      { label: "审计事件", path: "/admin/audit", icon: Activity },
    ],
  },
  {
    label: "服务器能力",
    items: [
      { label: "执行配置", path: "/admin/execution-profiles", icon: ServerCog },
      { label: "服务器凭据", path: "/admin/server-credentials", icon: KeyRound },
      { label: "健康检查", path: "/admin/credential-health", icon: HeartPulse },
      { label: "知识库包", path: "/admin/knowledge-packs", icon: Library },
    ],
  },
  {
    label: "用户与额度",
    items: [
      { label: "退款记录", path: "/admin/refunds", icon: ReceiptText },
      { label: "用户与额度", path: "/admin/users", icon: UsersRound },
    ],
  },
];

function adminLinkClass({ isActive }: { isActive: boolean }) {
  return [
    "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition",
    isActive ? "bg-white text-violet-950 shadow-sm" : "text-violet-100/80 hover:bg-white/10 hover:text-white",
  ].join(" ");
}

export function AdminLayout() {
  return (
    <div className="min-h-screen bg-slate-100 text-slate-900">
      <div className="grid min-h-screen lg:grid-cols-[256px_1fr]">
        <aside className="bg-violet-950 px-4 py-5 text-white lg:min-h-screen">
          <div className="flex items-center gap-3 px-2">
            <div className="grid h-10 w-10 place-items-center rounded-lg bg-white text-sm font-bold text-violet-950">
              SF
            </div>
            <div>
              <p className="text-base font-semibold">SpireForge 管理台</p>
              <p className="text-xs text-violet-100/70">Admin Console</p>
            </div>
          </div>

          <nav className="mt-8 space-y-6" aria-label="管理端导航">
            {adminNavGroups.map((group) => (
              <div key={group.label} className="space-y-2">
                <p className="px-3 text-[11px] font-semibold uppercase text-violet-100/50">{group.label}</p>
                <div className="space-y-1">
                  {group.items.map((item) => (
                    <NavLink key={item.path} to={item.path} end={item.path === "/admin"} className={adminLinkClass}>
                      <item.icon size={16} aria-hidden="true" />
                      <span>{item.label}</span>
                    </NavLink>
                  ))}
                </div>
              </div>
            ))}
          </nav>
        </aside>

        <main className="min-w-0 px-4 py-5 sm:px-6 lg:px-8">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export default AdminLayout;
