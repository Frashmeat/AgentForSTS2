// Log Analysis 页：粘贴 build log → LLM 出诊断 markdown。
// 大文本区 + 流式渲染。

import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import type { Job, JobProgressEvent, SubmitJobAck } from "@/services/tauriApi";

export function LogAnalysisPage() {
  const [logText, setLogText] = useState("");
  const [contextHint, setContextHint] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [stream, setStream] = useState("");
  const [report, setReport] = useState<string | null>(null);
  const jobIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    jobIdRef.current = jobId;
  }, [jobId]);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const ev = e.payload;
        if (ev.jobId !== jobIdRef.current) return;
        if (ev.delta) setStream((prev) => prev + ev.delta);
        if (ev.stage === "completed") {
          void (async () => {
            try {
              const job = (await api.getJob(ev.jobId)) as Job;
              const r = job.result as { report?: string } | null;
              setReport(r?.report ?? null);
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

  async function handleSubmit() {
    if (!logText.trim()) {
      setError("把 build log 贴进文本框");
      return;
    }
    setBusy(true);
    setError(null);
    setStream("");
    setReport(null);
    try {
      const ack = (await api.submitLogAnalysisJob({
        log_text: logText.trim(),
        context_hint: contextHint.trim() || null,
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
        <h1 className="text-2xl font-semibold mb-2">Log Analysis</h1>
        <p className="text-muted text-sm">桌面端 only。</p>
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-2xl font-semibold">Log Analysis</h1>
        <p className="text-muted text-sm">
          粘贴 dotnet publish 失败日志 → LLM 诊断 markdown
        </p>
      </header>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      <section className="rounded border border-muted/30 p-4 space-y-2">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Build log</span>
          <textarea
            value={logText}
            onChange={(e) => setLogText(e.target.value)}
            rows={12}
            placeholder="MSBUILD : error MSB4019: ..."
            className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Context hint（可选）</span>
          <input
            value={contextHint}
            onChange={(e) => setContextHint(e.target.value)}
            placeholder="我刚改了 TargetFramework 到 net9.0"
            className="px-2 py-1 rounded border border-muted/30 bg-transparent"
          />
        </label>
        <button
          type="button"
          onClick={handleSubmit}
          disabled={busy}
          className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
        >
          {busy ? "Submitting…" : "Diagnose"}
        </button>
      </section>

      {(stream || report) && (
        <section className="rounded border border-muted/30 p-4 space-y-2">
          <h2 className="text-lg font-medium">Diagnosis</h2>
          {jobId && (
            <p className="text-xs text-muted">
              Job <code>{jobId.slice(0, 8)}</code>
            </p>
          )}
          {report ? (
            <pre className="text-sm p-3 rounded border border-emerald-500/30 bg-emerald-50/30 whitespace-pre-wrap">
              {report}
            </pre>
          ) : (
            <pre className="text-xs p-3 rounded border border-muted/20 max-h-96 overflow-auto whitespace-pre-wrap bg-muted/5">
              {stream || "等待 LLM 首帧…"}
            </pre>
          )}
        </section>
      )}
    </div>
  );
}
