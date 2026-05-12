// AuditCard：展示当前 active project 的 audit.log 最近 N 条事件。
//
// 数据流：auditReadRecent(limit) → 倒序排（最新在前）。监听 job-progress
// 在 completed/failed/cancelled stage 自动 refresh，让用户看到 job 终态写盘
// 的 audit 立即出现，不用手点。

import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import type { AuditEntry, JobProgressEvent } from "@/services/tauriApi";

const DEFAULT_LIMIT = 20;

function kindColor(kind: string): string {
  switch (kind) {
    case "job.submitted":
      return "text-muted";
    case "job.started":
      return "text-accent";
    case "job.completed":
      return "text-emerald-600";
    case "job.failed":
      return "text-red-600";
    case "job.cancelled":
      return "text-amber-600";
    default:
      return "text-fg";
  }
}

function fmtTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString();
  } catch {
    return iso;
  }
}

export function AuditCard() {
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const unlistenRef = useRef<(() => void) | null>(null);

  async function refresh() {
    setBusy(true);
    setError(null);
    try {
      const rows = await api.auditReadRecent(limit);
      setEntries(rows as AuditEntry[]);
    } catch (e: unknown) {
      // 没有 active project / 文件不存在都会进这里，UI 不当 fatal 显
      setEntries([]);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void refresh();
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const stage = e.payload.stage;
        // 终态事件后自动拉一次最新审计
        if (
          stage === "completed" ||
          stage === "failed" ||
          stage === "cancelled-mid-stream"
        ) {
          void refresh();
        }
      });
      unlistenRef.current = stop;
    })();
    return () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [limit]);

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Audit</h2>
        <p className="text-muted text-sm">
          Audit 日志是桌面端 only —— Web 模式在 Stage 3 服务端审计落地后再启用。
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-2">
        <h2 className="text-lg font-medium">Audit · 最近事件</h2>
        <div className="flex items-center gap-2 text-sm">
          <label className="flex items-center gap-1 text-xs text-muted">
            limit
            <select
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="px-1 py-0.5 rounded border border-muted/30 bg-transparent"
            >
              <option value={20}>20</option>
              <option value={50}>50</option>
              <option value={100}>100</option>
            </select>
          </label>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={busy}
            className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
          >
            {busy ? "…" : "Refresh"}
          </button>
        </div>
      </div>

      {error && (
        <p className="text-xs text-muted mb-2">
          ({error.includes("active project") ? "先打开一个工程" : error})
        </p>
      )}

      {entries === null && !error && <p className="text-muted text-sm">Loading…</p>}
      {entries !== null && entries.length === 0 && !error && (
        <p className="text-muted text-sm">
          还没有 audit 事件。提交一个 job 后这里会自动出现 submitted/started/completed/failed。
        </p>
      )}

      {entries !== null && entries.length > 0 && (
        <ul className="space-y-1 text-xs font-mono max-h-72 overflow-auto">
          {entries.map((e, i) => (
            <li
              key={`${e.timestamp}-${i}`}
              className="grid grid-cols-[80px_140px_1fr] gap-2 items-baseline border-b border-muted/10 py-0.5"
            >
              <span className="text-muted">{fmtTime(e.timestamp)}</span>
              <span className={`font-medium ${kindColor(e.kind)}`}>{e.kind}</span>
              <span className="break-all">
                {e.message}
                {e.refId && (
                  <span className="text-muted ml-2">({e.refId.slice(0, 8)})</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
