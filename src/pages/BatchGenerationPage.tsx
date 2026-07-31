// 批量生成审查页：用户填多个 CustomCodegenRequest item 然后批量执行。
// 比 RunsCard 里裸 JSON textarea 友好：表格式增删改查。

import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Notice,
  PageHero,
} from "@/components/ui";
import { useProjectStore } from "@/stores/project";
import { useRunProgress } from "@/hooks/useRunProgress";
import { api } from "@/services/api";
import type {
  CustomCodegenRequest,
  RunRecord,
  SubmitRunAck,
} from "@/services/tauriApi";

interface BatchItem {
  name: string;
  description: string;
  implementation_notes: string;
}

function emptyItem(): BatchItem {
  return { name: "", description: "", implementation_notes: "" };
}

export function BatchGenerationPage() {
  const project = useProjectStore((s) => s.project);
  const [items, setItems] = useState<BatchItem[]>([emptyItem()]);
  const [failFast, setFailFast] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  const [run, setRun] = useState<RunRecord | null>(null);
  const [delta, setDelta] = useState("");
  const runIdRef = useRef<string | null>(null);

  useEffect(() => {
    runIdRef.current = runId;
  }, [runId]);

  useRunProgress(runIdRef, (ev) => {
    if (ev.delta) setDelta((prev) => prev + ev.delta);
    if (
      ev.stage === "completed" ||
      ev.stage === "failed" ||
      ev.stage === "item-failed" ||
      ev.stage.includes("error")
    ) {
      void (async () => {
        try {
          const next = (await api.getRun(ev.runId)) as RunRecord;
          setRun(next);
          if (next.status === "failed" && next.failure) {
            setError(`批量失败：${next.failure.message}`);
          }
        } catch {
          // ignore
        }
      })();
    }
  });

  function updateItem(idx: number, patch: Partial<BatchItem>) {
    setItems((prev) => prev.map((it, i) => (i === idx ? { ...it, ...patch } : it)));
  }

  async function handleSubmit() {
    if (!project) {
      setError("先打开一个工程");
      return;
    }
    const validItems = items.filter((it) => it.name.trim() !== "");
    if (validItems.length === 0) {
      setError("至少一个 item 需要 name");
      return;
    }
    const blankBodies = validItems.filter(
      (it) => !it.description.trim() && !it.implementation_notes.trim(),
    );
    if (blankBodies.length === validItems.length) {
      setError(
        "所有 item 都缺 description 与 implementation_notes —— LLM 会拿到空 prompt 并大概率失败。先填一份。",
      );
      return;
    }
    setBusy(true);
    setError(null);
    setDelta("");
    setRun(null);
    try {
      const req: CustomCodegenRequest[] = validItems.map((it) => ({
        name: it.name.trim(),
        description: it.description,
        implementation_notes: it.implementation_notes,
        project_root: project.path,
        skip_build: true,
      }));
      const ack = (await api.submitBatchCustomCodeRun({
        items: req,
        fail_fast: failFast,
      })) as SubmitRunAck;
      setRunId(ack.runId);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!__IS_TAURI__) {
    return (
      <div>
        <PageHero
          eyebrow="batch · custom code"
          title="Batch Generation"
          subtitle="桌面端 only。"
        />
      </div>
    );
  }

  const runVariant =
    run?.status === "succeeded"
      ? "ok"
      : run?.status === "failed"
        ? "error"
        : run?.status === "running"
          ? "running"
          : "muted";

  return (
    <div>
      <PageHero
        eyebrow="batch · custom code"
        title="Batch Generation"
        subtitle="批量跑 custom_code handler。每行一个 item，全部用同一个工程目录。"
      />

      <div className="space-y-4">
        {!project && (
          <Notice variant="warn" title="没有 active project">
            去 Dashboard 打开一个工程。
          </Notice>
        )}
        {error && <Notice variant="error" title={`Error: ${error}`} />}

        <Card eyebrow="items · request rows" title="Items">
          <div className="space-y-2">
            {items.map((it, i) => (
              <div
                key={i}
                className="grid items-start gap-2 p-2.5"
                style={{
                  gridTemplateColumns: "1fr 1fr 2fr auto",
                  background: "var(--paper)",
                  border: "1px solid var(--rule-soft)",
                  borderRadius: "3px",
                }}
              >
                <input
                  placeholder="name (类名)"
                  value={it.name}
                  onChange={(e) => updateItem(i, { name: e.target.value })}
                />
                <input
                  placeholder="description"
                  value={it.description}
                  onChange={(e) => updateItem(i, { description: e.target.value })}
                />
                <textarea
                  placeholder="implementation_notes"
                  value={it.implementation_notes}
                  onChange={(e) =>
                    updateItem(i, { implementation_notes: e.target.value })
                  }
                  rows={2}
                />
                <Button
                  size="sm"
                  onClick={() =>
                    setItems((prev) => prev.filter((_, k) => k !== i))
                  }
                  disabled={items.length === 1}
                >
                  ✕
                </Button>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-3 flex-wrap mt-3">
            <Button
              size="sm"
              onClick={() => setItems((prev) => [...prev, emptyItem()])}
            >
              + Add item
            </Button>
            <label
              className="flex items-center gap-2"
              style={{ fontSize: "13px" }}
            >
              <input
                type="checkbox"
                checked={failFast}
                onChange={(e) => setFailFast(e.target.checked)}
              />
              <span>fail_fast</span>
            </label>
            <Button
              variant="primary"
              onClick={handleSubmit}
              disabled={busy || !project}
            >
              {busy ? "Submitting…" : "Run batch"}
            </Button>
          </div>
        </Card>

        {runId && (
          <Card
            eyebrow="run · batch_custom_code"
            title="Run"
            actions={
              <>
                <code style={{ fontSize: "11.5px" }}>{runId.slice(0, 12)}…</code>
                {run && <Badge variant={runVariant}>{run.status}</Badge>}
              </>
            }
          >
            {delta && (
              <pre className="pre-block pre-block-stream max-h-48 mb-3">
                {delta}
              </pre>
            )}
            {run?.result !== undefined && run.result !== null && (
              <details open>
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
                  Result summary
                </summary>
                <pre className="pre-block max-h-64">
                  {JSON.stringify(run.result, null, 2)}
                </pre>
              </details>
            )}
          </Card>
        )}
      </div>
    </div>
  );
}
