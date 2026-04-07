import type { ReactNode } from "react";
import { Settings, Swords, type LucideIcon } from "lucide-react";

import { cn } from "../../lib/utils";
import { UserEntry } from "../UserEntry.tsx";

export interface WorkspaceNavItem<T extends string = string> {
  id: T;
  label: string;
  shortLabel: string;
  description: string;
  icon: LucideIcon;
}

export function WorkspaceShell<T extends string>({
  activeTab,
  navItems,
  onTabChange,
  onOpenSettings,
  children,
}: {
  activeTab: T;
  navItems: WorkspaceNavItem<T>[];
  onTabChange: (tab: T) => void;
  onOpenSettings: () => void;
  children: ReactNode;
}) {
  const activeItem = navItems.find((item) => item.id === activeTab) ?? navItems[0];

  return (
    <div className="workspace-shell">
      <aside className="workspace-sidebar">
        <div className="workspace-brand-mark">
          <Swords size={22} />
        </div>
        <div className="workspace-sidebar-nav">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onTabChange(item.id)}
                className={cn("workspace-sidebar-button", item.id === activeTab && "workspace-sidebar-button-active")}
                title={item.label}
                aria-label={item.label}
              >
                <Icon size={18} />
              </button>
            );
          })}
        </div>
        <button type="button" onClick={onOpenSettings} className="workspace-sidebar-button workspace-sidebar-footer" aria-label="打开设置" title="打开设置">
          <Settings size={18} />
        </button>
      </aside>

      <div className="workspace-main">
        <header className="workspace-topbar">
          <div className="workspace-topbar-copy">
            <div className="workspace-kicker">AgentTheSpire Workspace</div>
            <h1>{activeItem.label}</h1>
            <p>{activeItem.description}</p>
          </div>
          <div className="workspace-topbar-actions">
            <div className="workspace-upgrade-pill">控制台模式</div>
            <button type="button" onClick={onOpenSettings} className="workspace-action-button">
              <Settings size={15} />
              <span>设置</span>
            </button>
            <div className="workspace-user-entry">
              <UserEntry />
            </div>
          </div>
        </header>

        <div className="workspace-mobile-tabs" role="tablist" aria-label="工作区导航">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={item.id === activeTab}
                onClick={() => onTabChange(item.id)}
                className={cn("workspace-mobile-tab", item.id === activeTab && "workspace-mobile-tab-active")}
              >
                <Icon size={15} />
                <span>{item.shortLabel}</span>
              </button>
            );
          })}
        </div>

        <main className="workspace-content">{children}</main>
      </div>
    </div>
  );
}
