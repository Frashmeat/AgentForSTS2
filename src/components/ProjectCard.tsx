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

      {recents.length > 0 && (
        <div>
          <p className="text-sm font-medium mb-2">Recent</p>
          <ul className="space-y-1 text-sm">
            {recents.map((r) => (
              <li
                key={r.path}
                className="flex items-center justify-between border border-muted/20 rounded p-2 gap-2"
              >
                <div className="min-w-0 flex-1">
                  <p className="font-medium truncate">{r.name}</p>
                  <p className="text-xs text-muted truncate">
                    <code>{r.path}</code>
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => handleOpen(r.path)}
                  disabled={busy}
                  className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
                >
                  Open
                </button>
                <button
                  type="button"
                  onClick={() => handleForget(r.path)}
                  disabled={busy}
                  className="text-xs px-2 py-1 rounded border border-muted/20 text-muted hover:bg-muted/10 disabled:opacity-50"
                >
                  Forget
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
