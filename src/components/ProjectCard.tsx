import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { ProjectSnapshot, RecentEntry } from "@/services/tauriApi";

export function ProjectCard() {
  const [current, setCurrent] = useState<ProjectSnapshot | null>(null);
  const [recents, setRecents] = useState<RecentEntry[]>([]);
  const [parentDir, setParentDir] = useState("");
  const [newName, setNewName] = useState("my_mod");
  const [openPath, setOpenPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      const [cur, recs] = await Promise.all([
        api.currentProject() as Promise<ProjectSnapshot | null>,
        api.listRecentProjects() as Promise<RecentEntry[]>,
      ]);
      setCurrent(cur);
      setRecents(recs);
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function handleCreate() {
    if (!parentDir.trim() || !newName.trim()) {
      setError("Parent dir and name required");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const snap = (await api.createProject(parentDir, newName)) as ProjectSnapshot;
      setCurrent(snap);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleOpen(path: string) {
    setBusy(true);
    setError(null);
    try {
      const snap = (await api.openProject(path)) as ProjectSnapshot;
      setCurrent(snap);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleClose() {
    setBusy(true);
    setError(null);
    try {
      await api.closeProject();
      setCurrent(null);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleForget(path: string) {
    try {
      await api.forgetRecentProject(path);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Project</h2>
        <p className="text-muted text-sm">
          Project folders are desktop-only. In Web mode, projects live in the
          server database instead.
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">Project</h2>
        {current && (
          <button
            type="button"
            onClick={handleClose}
            disabled={busy}
            className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
          >
            Close
          </button>
        )}
      </div>

      {error && <p className="text-red-500 text-sm mb-2">Error: {error}</p>}

      {current ? (
        <div className="mb-4">
          <p className="text-sm">
            <span className="text-muted">Active:</span>{" "}
            <span className="font-medium">{current.meta.name}</span>
          </p>
          <p className="text-xs text-muted">
            <code>{current.path}</code>
          </p>
          <p className="text-xs text-muted mt-1">
            Created:{" "}
            <span className="font-mono">
              {new Date(current.meta.created_at).toLocaleString()}
            </span>{" "}
            · Schema v{current.meta.schema_version}
          </p>
        </div>
      ) : (
        <p className="text-muted text-sm mb-4">No active project</p>
      )}

      <details open className="mb-3">
        <summary className="cursor-pointer text-sm font-medium mb-2">
          Create new project
        </summary>
        <div className="space-y-2 pl-2">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Parent directory</span>
            <input
              value={parentDir}
              onChange={(e) => setParentDir(e.target.value)}
              placeholder="E:/mods"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Project name</span>
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
          <button
            type="button"
            onClick={handleCreate}
            disabled={busy}
            className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </details>

      <details className="mb-3">
        <summary className="cursor-pointer text-sm font-medium mb-2">
          Open existing project (path)
        </summary>
        <div className="flex gap-2 pl-2">
          <input
            value={openPath}
            onChange={(e) => setOpenPath(e.target.value)}
            placeholder="E:/mods/my_mod"
            className="flex-1 px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => handleOpen(openPath)}
            disabled={busy || !openPath.trim()}
            className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
          >
            Open
          </button>
        </div>
      </details>

      <div>
        <div className="flex items-center justify-between mb-2">
          <p className="text-sm font-medium">Recent ({recents.length})</p>
          {recents.length > 0 && (
            <button
              type="button"
              onClick={() => void refresh()}
              disabled={busy}
              className="text-xs px-2 py-0.5 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
            >
              Refresh
            </button>
          )}
        </div>
        {recents.length === 0 ? (
          <p className="text-xs text-muted">
            还没打开过任何工程。上面"Create" 建一个新工程，或 "Open existing
            project" 输入既有工程路径打开。
          </p>
        ) : (
          <ul className="space-y-1 text-sm">
            {recents.map((r) => {
              const isActive = current?.path === r.path;
              return (
                <li
                  key={r.path}
                  className={`flex items-center justify-between border rounded p-2 gap-2 ${
                    isActive
                      ? "border-emerald-500/40 bg-emerald-50/20"
                      : "border-muted/20 hover:bg-muted/5"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => !isActive && handleOpen(r.path)}
                    disabled={busy || isActive}
                    className="min-w-0 flex-1 text-left disabled:cursor-default"
                  >
                    <p className="font-medium truncate flex items-center gap-2">
                      {r.name}
                      {isActive && (
                        <span className="text-xs text-emerald-600">(active)</span>
                      )}
                    </p>
                    <p className="text-xs text-muted truncate">
                      <code>{r.path}</code>
                    </p>
                    <p className="text-xs text-muted">
                      Last opened:{" "}
                      <span className="font-mono">
                        {fmtRelative(r.last_opened_at)}
                      </span>
                    </p>
                  </button>
                  {!isActive && (
                    <button
                      type="button"
                      onClick={() => handleOpen(r.path)}
                      disabled={busy}
                      className="text-xs px-2 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
                    >
                      Open
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => handleForget(r.path)}
                    disabled={busy}
                    className="text-xs px-2 py-1 rounded border border-muted/20 text-muted hover:bg-muted/10 disabled:opacity-50"
                    title="从最近列表移除（不删除文件）"
                  >
                    Forget
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}

/// 把 ISO 时间字串渲染为"x 分钟/小时/天前"，>30 天显示本地日期。
function fmtRelative(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diffMs = Date.now() - t;
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day}d ago`;
  return new Date(t).toLocaleDateString();
}
