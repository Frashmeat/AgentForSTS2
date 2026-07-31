// Log Analysis 页：粘贴 build log → LLM 出诊断 markdown。
// 大文本区 + 流式渲染。

import { useEffect, useRef, useState } from "react";
import { useRunProgress } from "@/hooks/useRunProgress";
import { Button, Card, Field, PageHero } from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { api } from "@/services/api";
import {
  localValidationFailure,
  toActionableFailure,
} from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type { RunRecord, SubmitRunAck } from "@/services/tauriApi";

export function LogAnalysisPage() {
  const [logText, setLogText] = useState("");
  const [contextHint, setContextHint] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ActionableFailure | null>(null);
  const [runId, setRunId] = useState<string | null>(null);
  const [stream, setStream] = useState("");
  const [report, setReport] = useState<string | null>(null);
  const runIdRef = useRef<string | null>(null);

  useEffect(() => {
    runIdRef.current = runId;
  }, [runId]);

  useRunProgress(runIdRef, (ev) => {
    if (ev.delta) setStream((prev) => prev + ev.delta);
    if (ev.stage === "completed") {
      setBusy(false);
      void (async () => {
        try {
          const run = await api.getRun(ev.runId) as RunRecord;
          const content = (run.result as { content?: string } | null)?.content ?? "";
          setReport(content);
        } catch (err) { setError(toActionableFailure(err)); }
      })();
    }
    if (ev.stage === "failed" || ev.stage.includes("error")) {
      setBusy(false);
      void (async () => {
        try {
          const run = (await api.getRun(ev.runId)) as RunRecord;
          setError(run.failure ?? localValidationFailure(
            "log_analysis.run",
            "Log analysis failed without a valid failure payload.",
          ));
        } catch (caught: unknown) {
          setError(toActionableFailure(caught));
        }
      })();
    }
  });

  async function handleSubmit() {
    if (!logText.trim()) {
      setError(localValidationFailure(
        "log_analysis.input",
        "把 build log 贴进文本框。",
      ));
      return;
    }
    setBusy(true);
    setError(null);
    setStream("");
    setReport(null);
    try {
      const ack = (await api.submitLogAnalysisRun({
        log_text: logText.trim(),
        context_hint: contextHint.trim() || null,
      })) as SubmitRunAck;
      setRunId(ack.runId);
    } catch (e: unknown) {
      setError(toActionableFailure(e));
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
        <ActionableErrorNotice failure={error} />

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
              runId && (
                <code style={{ fontSize: "11.5px" }}>{runId.slice(0, 12)}</code>
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
