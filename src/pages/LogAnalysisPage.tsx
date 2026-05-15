// Log Analysis 页：粘贴 build log → LLM 出诊断 markdown。
// 大文本区 + 流式渲染。

import { useEffect, useRef, useState } from "react";
import { Button, Card, Field, Notice, PageHero } from "@/components/ui";
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
        } else if (ev.stage === "failed" || ev.stage.includes("error")) {
          void (async () => {
            try {
              const job = (await api.getJob(ev.jobId)) as Job;
              setError(`LLM 诊断失败：${job.error ?? ev.message ?? ev.stage}`);
            } catch {
              setError(`LLM 诊断失败（stage=${ev.stage}）：${ev.message ?? ""}`);
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
      <div>
        <PageHero
          eyebrow="diagnostics · log"
          title="Log Analysis"
          subtitle="桌面端 only。"
        />
      </div>
    );
  }

  return (
    <div>
      <PageHero
        eyebrow="diagnostics · log analysis"
        title="Log Analysis"
        subtitle="粘贴 dotnet publish 失败日志 → LLM 诊断 markdown"
      />

      <div className="space-y-4">
        {error && <Notice variant="error" title={`Error: ${error}`} />}

        <Card eyebrow="input · build log" title="Submit log">
          <div className="space-y-3">
            <Field label="build log">
              <textarea
                value={logText}
                onChange={(e) => setLogText(e.target.value)}
                rows={12}
                placeholder="MSBUILD : error MSB4019: ..."
                className="input-mono"
              />
            </Field>
            <Field label="context hint（可选）">
              <input
                value={contextHint}
                onChange={(e) => setContextHint(e.target.value)}
                placeholder="我刚改了 TargetFramework 到 net9.0"
              />
            </Field>
            <Button variant="primary" onClick={handleSubmit} disabled={busy}>
              {busy ? "Submitting…" : "Diagnose"}
            </Button>
          </div>
        </Card>

        {(stream || report) && (
          <Card
            eyebrow="output · llm diagnosis"
            title="Diagnosis"
            actions={
              jobId && (
                <code style={{ fontSize: "11.5px" }}>{jobId.slice(0, 12)}</code>
              )
            }
          >
            {report ? (
              <pre className="pre-block pre-block-ok">{report}</pre>
            ) : (
              <pre className="pre-block pre-block-stream max-h-96">
                {stream || "等待 LLM 首帧…"}
              </pre>
            )}
          </Card>
        )}
      </div>
    </div>
  );
}
