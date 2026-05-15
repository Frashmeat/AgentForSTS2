// AuditCard：展示当前 active project 的 audit.log 最近 N 条事件。
//
// 数据流：auditReadRecent(limit) → 倒序排（最新在前）。监听 job-progress
// 在 completed/failed/cancelled stage 自动 refresh，让用户看到 job 终态写盘
// 的 audit 立即出现，不用手点。

import { useEffect, useMemo, useRef, useState } from "react";
import { Badge, Button, Card, Field } from "@/components/ui";
import { api } from "@/services/api";
import type {
  AuditEntry,
  JobProgressEvent,
  PrewarmStatus,
} from "@/services/tauriApi";

const DEFAULT_LIMIT = 20;
const KIND_OPTIONS = [
  "all",
  "job.submitted",
  "job.started",
  "job.completed",
  "job.failed",
  "job.cancelled",
] as const;

type KindFilter = (typeof KIND_OPTIONS)[number];

function prewarmBadge(
  s: PrewarmStatus,
): { text: string; variant: "muted" | "warn" | "ok" } {
  switch (s.state) {
    case "idle":
      return { text: "ML rembg · idle", variant: "muted" };
    case "loading":
      return {
        text: `ML rembg · loading — ${s.message}`,
        variant: "warn",
      };
    case "ready":
      return {
        text: `ML rembg · ready (${s.model})`,
        variant: "ok",
      };
    case "failed":
      return {
        text: `ML rembg · fallback — ${s.message}`,
        variant: "muted",
      };
    default:
      return { text: "ML rembg · ?", variant: "muted" };
  }
}

function kindColor(kind: string): string {
  switch (kind) {
    case "job.submitted":
      return "var(--ink-mute)";
    case "job.started":
      return "var(--accent)";
    case "job.completed":
      return "var(--jade)";
    case "job.failed":
      return "var(--accent-deep)";
    case "job.cancelled":
      return "var(--gold)";
    default:
      return "var(--ink)";
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
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [prewarm, setPrewarm] = useState<PrewarmStatus>({ state: "idle" });
  const unlistenRef = useRef<(() => void) | null>(null);

  const visibleEntries = useMemo(() => {
    if (!entries) return null;
    if (kindFilter === "all") return entries;
    return entries.filter((e) => e.kind === kindFilter);
  }, [entries, kindFilter]);

  async function refresh() {
    setBusy(true);
    setError(null);
    try {
      const rows = await api.auditReadRecent(limit);
      setEntries(rows as AuditEntry[]);
    } catch (e: unknown) {
      setEntries([]);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!__IS_TAURI__) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      try {
        const s = (await api.imageProcStatus()) as PrewarmStatus;
        if (stopped) return;
        setPrewarm(s);
        if (s.state === "loading" || s.state === "idle") {
          timer = setTimeout(() => void tick(), 2000);
        }
      } catch {
        // 静默
      }
    };
    void tick();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void refresh();
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const stage = e.payload.stage;
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
      <Card
        eyebrow="observability · audit"
        title="Audit"
        subtitle="Audit 日志是桌面端 only —— Web 模式在 Stage 3 服务端审计落地后再启用。"
      />
    );
  }

  const badge = prewarmBadge(prewarm);

  return (
    <Card
      eyebrow="observability · audit"
      title="Audit · 最近事件"
      actions={
        <>
          <Badge variant={badge.variant} title="ML 背景去除模型预热状态">
            {badge.text}
          </Badge>
          <Field label="kind">
            <select
              value={kindFilter}
              onChange={(e) => setKindFilter(e.target.value as KindFilter)}
            >
              {KIND_OPTIONS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
          </Field>
          <Field label="limit">
            <select
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
            >
              <option value={20}>20</option>
              <option value={50}>50</option>
              <option value={100}>100</option>
            </select>
          </Field>
          <Button size="sm" onClick={() => void refresh()} disabled={busy}>
            {busy ? "…" : "Refresh"}
          </Button>
        </>
      }
    >
      {error && (
        <p
          style={{ fontSize: "12px", color: "var(--ink-mute)" }}
          className="mb-2"
        >
          ({error.includes("active project") ? "先打开一个工程" : error})
        </p>
      )}

      {entries === null && !error && (
        <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>
      )}
      {entries !== null && entries.length === 0 && !error && (
        <p style={{ color: "var(--ink-faint)", fontSize: "12.5px" }}>
          还没有 audit 事件。提交一个 job 后这里会自动出现 submitted/started/completed/failed。
        </p>
      )}
      {visibleEntries !== null &&
        entries !== null &&
        entries.length > 0 &&
        visibleEntries.length === 0 && (
          <p style={{ color: "var(--ink-faint)", fontSize: "12.5px" }}>
            当前 filter 没有匹配事件（kind={kindFilter}）。
          </p>
        )}

      {visibleEntries !== null && visibleEntries.length > 0 && (
        <ul
          className="space-y-0.5 max-h-72 overflow-auto"
          style={{ fontSize: "11.5px", fontFamily: '"JetBrains Mono", monospace' }}
        >
          {visibleEntries.map((e, i) => (
            <li
              key={`${e.timestamp}-${i}`}
              className="grid items-baseline gap-2 py-1"
              style={{
                gridTemplateColumns: "80px 140px 1fr",
                borderBottom: "1px solid var(--rule-hair)",
              }}
            >
              <span style={{ color: "var(--ink-mute)" }}>{fmtTime(e.timestamp)}</span>
              <span style={{ color: kindColor(e.kind), fontWeight: 500 }}>
                {e.kind}
              </span>
              <span className="break-all" style={{ color: "var(--ink-soft)" }}>
                {e.message}
                {e.refId && (
                  <span style={{ color: "var(--ink-faint)", marginLeft: "8px" }}>
                    ({e.refId.slice(0, 8)})
                  </span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
