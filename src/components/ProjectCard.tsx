import { useEffect, useState } from "react";
import { useProjectStore } from "@/stores/project";
import { Badge, Button, Card, CardSection, Field, Notice } from "@/components/ui";
import { api } from "@/services/api";
import { INSTALLED_GAME_PACK_ID } from "@/services/gamePacks";
import type { RecentEntry } from "@/services/tauriApi";

export function ProjectCard() {
  const current = useProjectStore((s) => s.project);
  const [recents, setRecents] = useState<RecentEntry[]>([]);
  const [parentDir, setParentDir] = useState("");
  const [newName, setNewName] = useState("my_mod");
  const [openPath, setOpenPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      const recs = (await api.listRecentProjects()) as RecentEntry[];
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
      await api.createProject(parentDir, newName, INSTALLED_GAME_PACK_ID);
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
      await api.openProject(path);
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
      <Card
        eyebrow="workspace · project"
        title="Project"
        subtitle="Project folders are desktop-only. In Web mode, projects live in the server database instead."
      />
    );
  }

  return (
    <Card
      eyebrow="workspace · project"
      title="Project"
      actions={
        current && (
          <Button size="sm" onClick={handleClose} disabled={busy}>
            Close
          </Button>
        )
      }
    >
      {error && <Notice variant="error" title={`Error: ${error}`} />}

      {current ? (
        <div className="mb-4">
          <div className="flex items-center gap-2 flex-wrap mb-1">
            <Badge variant="ok">active</Badge>
            <span style={{ fontWeight: 500 }}>{current.meta.name}</span>
          </div>
          <p style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
            <code>{current.path}</code>
          </p>
          <p
            className="mt-1"
            style={{ fontSize: "11.5px", color: "var(--ink-faint)" }}
          >
            Created{" "}
            <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
              {new Date(current.meta.created_at).toLocaleString()}
            </span>
            {" · schema v"}
            {current.meta.schema_version}
            {" · game "}
            <code>{current.meta.game_id}</code>
          </p>
        </div>
      ) : (
        <p
          style={{ color: "var(--ink-mute)", fontSize: "13px" }}
          className="mb-4"
        >
          No active project
        </p>
      )}

      <details open className="mb-3">
        <summary
          className="cursor-pointer mb-2"
          style={{
            fontFamily: '"JetBrains Mono", monospace',
            fontSize: "10.5px",
            letterSpacing: "0.16em",
            textTransform: "uppercase",
            color: "var(--ink-mute)",
          }}
        >
          Create new project
        </summary>
        <div className="space-y-2 pl-1">
          <Field label="parent directory">
            <input
              value={parentDir}
              onChange={(e) => setParentDir(e.target.value)}
              placeholder="E:/mods"
              className="input-mono"
            />
          </Field>
          <Field label="project name">
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
          </Field>
          <Button variant="accent" onClick={handleCreate} disabled={busy}>
            Create
          </Button>
        </div>
      </details>

      <details className="mb-3">
        <summary
          className="cursor-pointer mb-2"
          style={{
            fontFamily: '"JetBrains Mono", monospace',
            fontSize: "10.5px",
            letterSpacing: "0.16em",
            textTransform: "uppercase",
            color: "var(--ink-mute)",
          }}
        >
          Open existing project (path)
        </summary>
        <div className="flex gap-2 pl-1 items-end">
          <Field label="path" className="flex-1">
            <input
              value={openPath}
              onChange={(e) => setOpenPath(e.target.value)}
              placeholder="E:/mods/my_mod"
              className="input-mono"
            />
          </Field>
          <Button
            onClick={() => handleOpen(openPath)}
            disabled={busy || !openPath.trim()}
          >
            Open
          </Button>
        </div>
      </details>

      <CardSection>
        <div className="flex items-center justify-between mb-2">
          <h3 style={{ margin: 0 }}>Recent ({recents.length})</h3>
          {recents.length > 0 && (
            <Button size="sm" onClick={() => void refresh()} disabled={busy}>
              Refresh
            </Button>
          )}
        </div>
        {recents.length === 0 ? (
          <p style={{ color: "var(--ink-faint)", fontSize: "12px" }}>
            还没打开过任何工程。上面 "Create" 建一个新工程，或 "Open existing project" 输入既有工程路径打开。
          </p>
        ) : (
          <ul className="space-y-1.5">
            {recents.map((r) => {
              const isActive = current?.path === r.path;
              return (
                <li
                  key={r.path}
                  className="flex items-center gap-2 p-2.5"
                  style={{
                    background: isActive
                      ? "rgba(77, 122, 106, 0.08)"
                      : "var(--paper)",
                    border: "1px solid",
                    borderColor: isActive
                      ? "rgba(77, 122, 106, 0.4)"
                      : "var(--rule-soft)",
                    borderRadius: "3px",
                  }}
                >
                  <button
                    type="button"
                    onClick={() => !isActive && handleOpen(r.path)}
                    disabled={busy || isActive}
                    className="min-w-0 flex-1 text-left disabled:cursor-default"
                    style={{
                      background: "transparent",
                      border: 0,
                      padding: 0,
                      color: "inherit",
                      cursor: isActive ? "default" : "pointer",
                    }}
                  >
                    <p
                      className="flex items-center gap-2"
                      style={{ fontWeight: 500 }}
                    >
                      <span className="truncate">{r.name}</span>
                      {isActive && <Badge variant="ok">active</Badge>}
                    </p>
                    <p
                      className="truncate"
                      style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}
                    >
                      <code>{r.path}</code>
                    </p>
                    <p
                      style={{ fontSize: "11px", color: "var(--ink-faint)" }}
                    >
                      Last opened{" "}
                      <span
                        style={{ fontFamily: '"JetBrains Mono", monospace' }}
                      >
                        {fmtRelative(r.last_opened_at)}
                      </span>
                    </p>
                  </button>
                  {!isActive && (
                    <Button
                      size="sm"
                      variant="accent"
                      onClick={() => handleOpen(r.path)}
                      disabled={busy}
                    >
                      Open
                    </Button>
                  )}
                  <Button
                    size="sm"
                    onClick={() => handleForget(r.path)}
                    disabled={busy}
                    title="从最近列表移除（不删除文件）"
                  >
                    Forget
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </CardSection>
    </Card>
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
