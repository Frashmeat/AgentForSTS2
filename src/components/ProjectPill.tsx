import { useEffect, useRef, useState } from "react";
import { useProjectStore } from "@/stores/project";
import { api } from "@/services/api";
import type { RecentEntry } from "@/services/tauriApi";

function useClickOutside(ref: React.RefObject<HTMLDivElement | null>, handler: () => void) {
  useEffect(() => {
    function listener(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) handler();
    }
    document.addEventListener("mousedown", listener);
    return () => document.removeEventListener("mousedown", listener);
  }, [ref, handler]);
}

export function ProjectPill() {
  const project = useProjectStore((s) => s.project);
  const [open, setOpen] = useState(false);
  const [recents, setRecents] = useState<RecentEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [parentDir, setParentDir] = useState("E:/mods");
  const [newName, setNewName] = useState("my_mod");
  const ref = useRef<HTMLDivElement>(null);

  useClickOutside(ref, () => { setOpen(false); setShowCreate(false); });

  async function refreshRecents() {
    try { setRecents((await api.listRecentProjects()) as RecentEntry[]); } catch {}
  }

  useEffect(() => { if (open) { void refreshRecents(); } }, [open]);

  async function handleOpen(path: string) {
    setBusy(true);
    try { await api.openProject(path); } catch (e) { console.error(e); }
    finally { setBusy(false); setOpen(false); }
  }

  async function handleCreate() {
    if (!parentDir.trim() || !newName.trim()) return;
    setBusy(true);
    try { await api.createProject(parentDir, newName); setShowCreate(false); setOpen(false); } catch (e) { console.error(e); }
    finally { setBusy(false); }
  }

  async function handleClose() {
    try { await api.closeProject(); } catch {}
    setOpen(false);
  }

  if (!__IS_TAURI__) return null;

  const label = project ? project.meta.name : "无工程";
  

  return (
    <div style={{ position: "relative" }} ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        disabled={busy}
        style={{
          display: "inline-flex", alignItems: "center", gap: "6px",
          padding: "3px 10px", borderRadius: "4px", border: "none", cursor: "pointer",
          fontSize: "11.5px", fontFamily: '"JetBrains Mono", monospace',
          background: "var(--paper-soft)", color: "var(--ink-mute)",
        }}
      >
        <span>📁</span>
        <span style={{ maxWidth: "140px", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
        <span style={{ fontSize: "9px" }}>▾</span>
      </button>

      {open && !showCreate && (
        <div style={{
          position: "absolute", top: "100%", right: 0, marginTop: "4px",
          minWidth: "240px", background: "var(--paper)", border: "1px solid var(--rule)",
          borderRadius: "6px", boxShadow: "0 4px 12px rgba(0,0,0,0.15)", zIndex: 100,
          padding: "8px",
        }}>
          {project && (
            <div style={{ padding: "6px 8px", borderBottom: "1px solid var(--rule-soft)", marginBottom: "4px" }}>
              <div style={{ fontSize: "12px", fontWeight: 600 }}>{project.meta.name}</div>
              <div style={{ fontSize: "10px", color: "var(--ink-mute)", overflow: "hidden", textOverflow: "ellipsis" }}>{project.path}</div>
            </div>
          )}

          {recents.length === 0 && !project && <div style={{ padding: "6px 8px", fontSize: "12px", color: "var(--ink-mute)" }}>无历史工程</div>}

          {recents.slice(0, 8).map((r) => (
            <div
              key={r.path}
              onClick={() => void handleOpen(r.path)}
              style={{
                padding: "5px 8px", cursor: "pointer", borderRadius: "3px",
                fontSize: "12px", display: "flex", justifyContent: "space-between",
                alignItems: "center",
                background: project?.path === r.path ? "var(--rule-soft)" : "transparent",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--rule-soft)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = project?.path === r.path ? "var(--rule-soft)" : "transparent"; }}
            >
              <span>{r.name}</span>
              <span style={{ fontSize: "10px", color: "var(--ink-faint)" }}>{r.path.split(/[/\\]/).slice(-2).join("/")}</span>
            </div>
          ))}

          <div style={{ borderTop: "1px solid var(--rule-soft)", marginTop: "4px", paddingTop: "4px" }}>
            <button
              onClick={() => setShowCreate(true)}
              style={{ width: "100%", padding: "6px 8px", border: "none", background: "none",
                cursor: "pointer", textAlign: "left", fontSize: "12px", color: "var(--accent)" }}
            >
              + 新建工程…
            </button>
            {project && (
              <button
                onClick={() => void handleClose()}
                style={{ width: "100%", padding: "6px 8px", border: "none", background: "none",
                  cursor: "pointer", textAlign: "left", fontSize: "12px", color: "var(--ink-mute)" }}
              >
                ✕ 关闭工程
              </button>
            )}
          </div>
        </div>
      )}

      {open && showCreate && (
        <div style={{
          position: "absolute", top: "100%", right: 0, marginTop: "4px",
          minWidth: "260px", background: "var(--paper)", border: "1px solid var(--rule)",
          borderRadius: "6px", boxShadow: "0 4px 12px rgba(0,0,0,0.15)", zIndex: 100,
          padding: "10px",
        }}>
          <div style={{ fontSize: "12px", fontWeight: 600, marginBottom: "8px" }}>新建工程</div>
          <div style={{ marginBottom: "6px" }}>
            <div style={{ fontSize: "10px", color: "var(--ink-mute)", marginBottom: "2px" }}>父目录</div>
            <input value={parentDir} onChange={(e) => setParentDir(e.target.value)}
              placeholder="E:/mods" style={{ width: "100%", padding: "4px 6px", fontSize: "12px",
                border: "1px solid var(--rule-soft)", borderRadius: "3px",
                background: "var(--paper-soft)", color: "var(--ink)" }} />
          </div>
          <div style={{ marginBottom: "8px" }}>
            <div style={{ fontSize: "10px", color: "var(--ink-mute)", marginBottom: "2px" }}>工程名</div>
            <input value={newName} onChange={(e) => setNewName(e.target.value)}
              placeholder="my_mod" style={{ width: "100%", padding: "4px 6px", fontSize: "12px",
                border: "1px solid var(--rule-soft)", borderRadius: "3px",
                background: "var(--paper-soft)", color: "var(--ink)" }} />
          </div>
          <div style={{ display: "flex", gap: "6px", justifyContent: "flex-end" }}>
            <button onClick={() => setShowCreate(false)}
              style={{ padding: "4px 10px", border: "1px solid var(--rule-soft)", borderRadius: "3px",
                background: "none", cursor: "pointer", fontSize: "12px", color: "var(--ink-mute)" }}>
              取消
            </button>
            <button onClick={() => void handleCreate()} disabled={busy}
              style={{ padding: "4px 10px", border: "none", borderRadius: "3px",
                background: "var(--accent)", cursor: "pointer", fontSize: "12px", color: "#fff" }}>
              创建
            </button>
          </div>
          <div style={{ marginTop: "6px" }}>
            <button onClick={() => setShowCreate(false)}
              style={{ background: "none", border: "none", cursor: "pointer", fontSize: "11px", color: "var(--ink-mute)" }}>
              ← 返回工程列表
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
