// 任务列表 + 详情：从原 JobsCard 独立出来。
//
// 自管：list / active / liveDeltaById / busy。监听 job-progress 自动 refresh。
// 父级通过 ref 触发 refresh（提交完后调）。

import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { api } from "@/services/api";
import type {
  Job,
  JobProgressEvent,
  JobStatus,
  JobSummary,
} from "@/services/tauriApi";

const STATUS_COLOR: Record<JobStatus, string> = {
  pending: "text-muted",
  running: "text-accent",
  completed: "text-emerald-600",
  failed: "text-red-600",
  cancelled: "text-amber-600",
};

export interface JobsListHandle {
  refresh: () => Promise<void>;
}

interface Props {
  onError: (msg: string) => void;
}

export const JobsList = forwardRef<JobsListHandle, Props>(function JobsList(
  { onError },
  ref,
) {
  const [list, setList] = useState<JobSummary[]>([]);
  const [active, setActive] = useState<Job | null>(null);
  const [liveDeltaById, setLiveDeltaById] = useState<Record<string, string>>({});
  const unlistenRef = useRef<(() => void) | null>(null);

  async function refresh() {
    try {
      const items = (await api.listJobs()) as JobSummary[];
      setList(items);
    } catch (e: unknown) {
      onError(String(e));
    }
  }

  useImperativeHandle(ref, () => ({ refresh }));

  useEffect(() => {
    void refresh();
    if (!__IS_TAURI__) return;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const ev = e.payload;
        if (ev.delta) {
          setLiveDeltaById((prev) => ({
            ...prev,
            [ev.jobId]: (prev[ev.jobId] ?? "") + ev.delta,
          }));
        }
        if (
          ev.stage === "completed" ||
          ev.stage.includes("error") ||
          ev.stage.includes("cancel") ||
          ev.stage === "failed"
        ) {
          void refresh();
          setActive((cur) => {
            if (cur && cur.id === ev.jobId) {
              void (async () => {
                try {
                  const next = (await api.getJob(ev.jobId)) as Job;
                  setActive(next);
                } catch {
                  // ignore
                }
              })();
            }
            return cur;
          });
        }
      });
      unlistenRef.current = stop;
    })();
    return () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleSelect(id: string) {
    try {
      const job = (await api.getJob(id)) as Job;
      setActive(job);
    } catch (e: unknown) {
      onError(String(e));
    }
  }

  async function handleCancel(id: string) {
    try {
      await api.cancelJob(id);
      await refresh();
    } catch (e: unknown) {
      onError(String(e));
    }
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <p className="text-sm font-medium">
          Recent jobs ({list.length})
        </p>
        <button
          type="button"
          onClick={() => void refresh()}
          className="text-xs px-2 py-0.5 rounded border border-muted/40 hover:bg-muted/10"
        >
          Refresh
        </button>
      </div>

      {list.length === 0 ? (
        <p className="text-muted text-sm">No jobs yet. Submit one above.</p>
      ) : (
        <ul className="space-y-2 text-sm mb-4">
          {list.map((j) => (
            <li
              key={j.id}
              className={`border rounded p-2 flex items-center justify-between gap-2 ${
                active?.id === j.id
                  ? "border-accent/60 bg-accent/5"
                  : "border-muted/20"
              }`}
            >
              <button
                type="button"
                onClick={() => handleSelect(j.id)}
                className="min-w-0 flex-1 text-left"
              >
                <p>
                  <code className="text-xs">{j.id.slice(0, 8)}…</code>
                  <span
                    className={`ml-2 text-xs font-medium ${STATUS_COLOR[j.status]}`}
                  >
                    {j.status}
                  </span>
                  <span className="ml-2 text-xs text-muted">{j.kind}</span>
                </p>
                <p className="text-xs text-muted">
                  Created {new Date(j.createdAt).toLocaleTimeString()}
                  {j.completedAt && (
                    <>
                      {" · "}Completed{" "}
                      {new Date(j.completedAt).toLocaleTimeString()}
                    </>
                  )}
                </p>
                {liveDeltaById[j.id] && j.status === "running" && (
                  <p className="text-xs text-accent truncate">
                    {liveDeltaById[j.id]}
                  </p>
                )}
              </button>
              {j.status === "running" && (
                <button
                  type="button"
                  onClick={() => handleCancel(j.id)}
                  className="text-xs px-2 py-1 rounded border border-amber-500/40 text-amber-600 hover:bg-amber-50/40"
                  title="发送 cancel 信号；流式任务会在下一个事件处真断网"
                >
                  Cancel
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {active && (
        <details open className="mt-3">
          <summary className="cursor-pointer text-sm font-medium mb-2 flex items-center justify-between">
            <span>
              Job {active.id.slice(0, 8)}…{" "}
              <span className={`text-xs ${STATUS_COLOR[active.status]}`}>
                ({active.status})
              </span>
            </span>
            <button
              type="button"
              onClick={() => setActive(null)}
              className="text-xs text-muted hover:text-foreground"
            >
              Close detail
            </button>
          </summary>
          {active.error && (
            <p className="text-xs text-red-600 mb-2 break-all">
              error: {active.error}
            </p>
          )}
          <pre className="text-xs p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {JSON.stringify(
              active.result ?? { error: active.error, status: active.status },
              null,
              2,
            )}
          </pre>
        </details>
      )}
    </div>
  );
});
