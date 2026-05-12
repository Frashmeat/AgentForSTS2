// 批量生成审查页：用户填多个 CustomCodegenRequest item 然后批量执行。
// 比 JobsCard 里裸 JSON textarea 友好：表格式增删改查。

import { useEffect, useRef, useState } from "react";
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
              // 整批失败时 job.error 有顶层信息；部分 item 失败靠 result.items 看
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
      <section className="rounded border border-muted/30 p-4">
        <h1 className="text-2xl font-semibold mb-2">Batch Generation</h1>
        <p className="text-muted text-sm">桌面端 only。</p>
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-2xl font-semibold">Batch Generation</h1>
        <p className="text-muted text-sm">
          批量跑 custom_code handler。每行一个 item，全部用同一个工程目录。
        </p>
      </header>

      {!project && (
        <p className="text-amber-600 text-sm">没有 active project；去 Dashboard 打开一个。</p>
      )}
      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      <section className="rounded border border-muted/30 p-4 space-y-2">
        <div className="space-y-2">
          {items.map((it, i) => (
            <div
              key={i}
              className="border border-muted/20 rounded p-2 grid grid-cols-[1fr_1fr_2fr_auto] gap-2 items-start"
            >
              <input
                placeholder="name (类名)"
                value={it.name}
                onChange={(e) => updateItem(i, { name: e.target.value })}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent text-sm"
              />
              <input
                placeholder="description"
                value={it.description}
                onChange={(e) => updateItem(i, { description: e.target.value })}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent text-sm"
              />
              <textarea
                placeholder="implementation_notes"
                value={it.implementation_notes}
                onChange={(e) =>
                  updateItem(i, { implementation_notes: e.target.value })
                }
                rows={2}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent text-sm"
              />
              <button
                type="button"
                onClick={() => setItems((prev) => prev.filter((_, k) => k !== i))}
                disabled={items.length === 1}
                className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
              >
                ✕
              </button>
            </div>
          ))}
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => setItems((prev) => [...prev, emptyItem()])}
            className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10"
          >
            + Add item
          </button>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={failFast}
              onChange={(e) => setFailFast(e.target.checked)}
            />
            <span>fail_fast</span>
          </label>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={busy || !project}
            className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
          >
            {busy ? "Submitting…" : "Run batch"}
          </button>
        </div>
      </section>

      {jobId && (
        <section className="rounded border border-muted/30 p-4 space-y-2">
          <p className="text-sm">
            <span className="text-muted">Job: </span>
            <code className="text-xs">{jobId}</code>
            {job && (
              <span
                className={`ml-3 font-medium ${
                  job.status === "completed"
                    ? "text-emerald-600"
                    : job.status === "failed"
                      ? "text-red-600"
                      : "text-accent"
                }`}
              >
                {job.status}
              </span>
            )}
          </p>
          {delta && (
            <pre className="text-xs p-2 rounded border border-muted/20 max-h-48 overflow-auto whitespace-pre-wrap bg-muted/5">
              {delta}
            </pre>
          )}
          {job?.result !== undefined && job.result !== null && (
            <details open className="text-xs">
              <summary className="cursor-pointer text-sm font-medium">
                Result summary
              </summary>
              <pre className="mt-1 p-2 rounded border border-muted/20 overflow-auto max-h-64 whitespace-pre-wrap">
                {JSON.stringify(job.result, null, 2)}
              </pre>
            </details>
          )}
        </section>
      )}
    </div>
  );
}
