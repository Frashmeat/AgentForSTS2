// 批量生成审查页：用户填多个 CustomCodegenRequest item 然后批量执行。
// 比 JobsCard 里裸 JSON textarea 友好：表格式增删改查。

import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Notice,
  PageHero,
} from "@/components/ui";
import { api } from "@/services/api";
import type {
  CustomCodegenRequest,
  Job,
  JobProgressEvent,
  ProjectSnapshot,
  SubmitJobAck,
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
  const [project, setProject] = useState<ProjectSnapshot | null>(null);
  const [items, setItems] = useState<BatchItem[]>([emptyItem()]);
  const [failFast, setFailFast] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [job, setJob] = useState<Job | null>(null);
  const [delta, setDelta] = useState("");
  const jobIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    jobIdRef.current = jobId;
  }, [jobId]);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        setProject((await api.currentProject()) as ProjectSnapshot | null);
      } catch (e: unknown) {
        console.warn("currentProject:", e);
      }
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const ev = e.payload;
        if (ev.jobId !== jobIdRef.current) return;
        if (ev.delta) setDelta((prev) => prev + ev.delta);
        if (
          ev.stage === "completed" ||
          ev.stage === "failed" ||
          ev.stage === "item-failed" ||
          ev.stage.includes("error")
        ) {
          void (async () => {
            try {
              const next = (await api.getJob(ev.jobId)) as Job;
              setJob(next);
              if (next.status === "failed" && next.error) {
                setError(`批量失败：${next.error}`);
              }
            } catch {
              // ignore
            }
          })();
        }
      });
      unlistenRef.current = stop;
    })();
    return () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

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
    setJob(null);
    try {
      const req: CustomCodegenRequest[] = validItems.map((it) => ({
        name: it.name.trim(),
        description: it.description,
        implementation_notes: it.implementation_notes,
        project_root: project.path,
        skip_build: true,
      }));
      const ack = (await api.submitBatchCustomCodeJob({
        items: req,
        fail_fast: failFast,
      })) as SubmitJobAck;
      setJobId(ack.jobId);
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

  const jobVariant =
    job?.status === "completed"
      ? "ok"
      : job?.status === "failed"
        ? "error"
        : job?.status === "running"
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

        {jobId && (
          <Card
            eyebrow="job · batch_custom_code"
            title="Run"
            actions={
              <>
                <code style={{ fontSize: "11.5px" }}>{jobId.slice(0, 12)}…</code>
                {job && <Badge variant={jobVariant}>{job.status}</Badge>}
              </>
            }
          >
            {delta && (
              <pre className="pre-block pre-block-stream max-h-48 mb-3">
                {delta}
              </pre>
            )}
            {job?.result !== undefined && job.result !== null && (
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
                  {JSON.stringify(job.result, null, 2)}
                </pre>
              </details>
            )}
          </Card>
        )}
      </div>
    </div>
  );
}
