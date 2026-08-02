// 任务列表 + 详情：从原 RunsCard 独立出来。
//
// 自管：list / active / liveDeltaById / busy。监听 run-progress 自动 refresh。
// 父级通过 ref 触发 refresh（提交完后调）。

import {
  useCallback,
  useEffect,
  forwardRef,
  useImperativeHandle,
  useState,
} from "react";
import { Badge, Button } from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { useAllRunProgress } from "@/hooks/useRunProgress";
import { api } from "@/services/api";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type {
  RunRecord,
  RunStatus,
  RunSummary,
} from "@/services/tauriApi";

const STATUS_VARIANT: Record<RunStatus, "muted" | "running" | "ok" | "error" | "warn"> = {
  pending: "muted",
  running: "running",
  succeeded: "ok",
  failed: "error",
  cancelled: "warn",
};

export interface RunsListHandle {
  refresh: () => Promise<void>;
}

interface Props {
  onError: (failure: ActionableFailure) => void;
}

export const RunsList = forwardRef<RunsListHandle, Props>(function RunsList(
  { onError },
  ref,
) {
  const [list, setList] = useState<RunSummary[]>([]);
  const [active, setActive] = useState<RunRecord | null>(null);
  const [liveDeltaById, setLiveDeltaById] = useState<Record<string, string>>({});

  const refresh = useCallback(async () => {
    try {
      const items = (await api.listRuns()) as RunSummary[];
      setList(items);
    } catch (e: unknown) {
      onError(toActionableFailure(e));
    }
  }, [onError]);

  useEffect(() => {
    if (!list.some((run) => run.status === "pending" || run.status === "running")) {
      return;
    }
    const timer = window.setInterval(() => void refresh(), 500);
    return () => window.clearInterval(timer);
  }, [list, refresh]);

  useImperativeHandle(ref, () => ({ refresh }));

  useAllRunProgress((ev) => {
    if (ev.delta) {
      setLiveDeltaById((prev) => ({
        ...prev,
        [ev.runId]: (prev[ev.runId] ?? "") + ev.delta,
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
        if (cur && cur.id === ev.runId) {
          void (async () => {
            try {
              const next = (await api.getRun(ev.runId)) as RunRecord;
              setActive(next);
            } catch { /* ignore */ }
          })();
        }
        return cur;
      });
    }
  });

  async function handleSelect(id: string) {
    try {
      const run = (await api.getRun(id)) as RunRecord;
      setActive(run);
    } catch (e: unknown) {
      onError(toActionableFailure(e));
    }
  }

  async function handleCancel(id: string) {
    try {
      await api.cancelRun(id);
      await refresh();
    } catch (e: unknown) {
      onError(toActionableFailure(e));
    }
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <h3 style={{ margin: 0 }}>Recent runs ({list.length})</h3>
        <Button size="sm" onClick={() => void refresh()}>
          Refresh
        </Button>
      </div>

      {list.length === 0 ? (
        <p style={{ color: "var(--ink-faint)", fontSize: "12.5px" }}>
          No runs yet. Submit one above.
        </p>
      ) : (
        <ul className="space-y-2 mb-4">
          {list.map((j) => (
            <li
              key={j.id}
              data-testid="run-row"
              data-run-id={j.id}
              data-run-kind={j.kind}
              data-run-status={j.status}
              className="flex items-center gap-3 p-2.5"
              style={{
                background:
                  active?.id === j.id
                    ? "rgba(201, 56, 43, 0.05)"
                    : "var(--paper)",
                border: "1px solid",
                borderColor:
                  active?.id === j.id
                    ? "rgba(201, 56, 43, 0.4)"
                    : "var(--rule-soft)",
                borderRadius: "3px",
              }}
            >
              <button
                type="button"
                onClick={() => handleSelect(j.id)}
                className="min-w-0 flex-1 text-left"
                style={{
                  background: "transparent",
                  border: 0,
                  padding: 0,
                  color: "inherit",
                  cursor: "pointer",
                }}
              >
                <div className="flex items-center gap-2 flex-wrap">
                  <code style={{ fontSize: "11px" }}>{j.id.slice(0, 8)}…</code>
                  <Badge variant={STATUS_VARIANT[j.status]}>{j.status}</Badge>
                  <span style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
                    {j.kind}
                  </span>
                </div>
                <p style={{ fontSize: "11px", color: "var(--ink-faint)" }}>
                  Created{" "}
                  <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                    {new Date(j.createdAt).toLocaleTimeString()}
                  </span>
                  {j.completedAt && (
                    <>
                      {" · completed "}
                      <span
                        style={{ fontFamily: '"JetBrains Mono", monospace' }}
                      >
                        {new Date(j.completedAt).toLocaleTimeString()}
                      </span>
                    </>
                  )}
                </p>
                {liveDeltaById[j.id] && j.status === "running" && (
                  <p
                    className="truncate"
                    style={{ fontSize: "11.5px", color: "var(--accent)" }}
                  >
                    {liveDeltaById[j.id]}
                  </p>
                )}
              </button>
              {j.status === "running" && (
                <Button
                  size="sm"
                  variant="danger"
                  onClick={() => handleCancel(j.id)}
                  title="发送 cancel 信号；流式任务会在下一个事件处真断网"
                >
                  Cancel
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}

      {active && (
        <details
          open
          className="mt-3"
          data-testid="run-detail"
          data-run-id={active.id}
        >
          <summary
            className="cursor-pointer mb-2 flex items-center justify-between gap-2"
            style={{
              fontFamily: '"JetBrains Mono", monospace',
              fontSize: "10.5px",
              letterSpacing: "0.16em",
              textTransform: "uppercase",
              color: "var(--ink-mute)",
            }}
          >
            <span className="flex items-center gap-2">
              <span>RunRecord {active.id.slice(0, 8)}…</span>
              <Badge variant={STATUS_VARIANT[active.status]}>{active.status}</Badge>
            </span>
            <button
              type="button"
              onClick={() => setActive(null)}
              style={{
                fontSize: "10px",
                color: "var(--ink-mute)",
                background: "transparent",
                border: 0,
                cursor: "pointer",
                letterSpacing: "0.16em",
                textTransform: "uppercase",
              }}
            >
              Close
            </button>
          </summary>
          {active.failure && (
            <div data-testid="run-detail-error">
              <ActionableErrorNotice failure={active.failure} className="mb-2" />
            </div>
          )}
          <pre className="pre-block max-h-96" data-testid="run-detail-result">
            {JSON.stringify(
              active.result ?? { failure: active.failure, status: active.status },
              null,
              2,
            )}
          </pre>
        </details>
      )}
    </div>
  );
});
