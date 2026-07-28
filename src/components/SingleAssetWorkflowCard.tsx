import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import { useJobProgress } from "@/hooks/useJobProgress";
import { useProjectStore } from "@/stores/project";
import { useWorkflowStore } from "@/stores/workflow";
import { Button, Card, CardSection, Field, Notice } from "@/components/ui";
import type {
  AssetItemType,
  Job,
  JobProgressEvent,
  PlanItem,
  SubmitJobAck,
} from "@/services/tauriApi";

const ASSET_TYPES: AssetItemType[] = [
  "card",
  "card_fullscreen",
  "relic",
  "power",
  "character",
  "custom_code",
] as const;

type Phase =
  | "idle"
  | "planning"
  | "plan_done"
  | "generating"
  | "code_done";

export function SingleAssetWorkflowCard() {
  const project = useProjectStore((s) => s.project);
  const wfStore = useWorkflowStore();

  const [requirements, setRequirements] = useState(
    "做一个回合开始时获得 3 点格挡的卡牌",
  );
  const [assetType, setAssetType] = useState<AssetItemType>("card");
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [planDelta, setPlanDelta] = useState("");
  const [planItem, setPlanItem] = useState<PlanItem | null>(null);
  const [codeDelta, setCodeDelta] = useState("");
  const [codeResult, setCodeResult] = useState<{
    csPath: string;
    artifactCsPath?: string | null;
    rawPath: string;
    extractedChars: number;
    pngPath?: string | null;
  } | null>(null);

  const planJobIdRef = useRef<string | null>(null);
  const codeJobIdRef = useRef<string | null>(null);

  planJobIdRef.current = wfStore.planJobId;
  codeJobIdRef.current = wfStore.codeJobId;

  useJobProgress(planJobIdRef, handlePlanProgress);
  useJobProgress(codeJobIdRef, handleCodeProgress);

  function clearPlanStore() { useWorkflowStore.getState().setPlanJobId(null); }
  function clearCodeStore() { useWorkflowStore.getState().setCodeJobId(null); }

  function handlePlanResult(job: Job) {
    const result = job.result as { item?: PlanItem } | null;
    if (result?.item) setPlanItem(result.item);
    setPhase("plan_done");
    clearPlanStore();
  }

  function handleCodeResult(job: Job) {
    const r = job.result as {
      csPath?: string; artifactCsPath?: string | null;
      rawPath?: string; extractedChars?: number; pngPath?: string | null;
    } | null;
    if (r?.csPath) setCodeResult({
      csPath: r.csPath, artifactCsPath: r.artifactCsPath ?? null,
      rawPath: r.rawPath ?? "", extractedChars: r.extractedChars ?? 0, pngPath: r.pngPath ?? null,
    });
    setPhase("code_done");
    clearCodeStore();
  }

  function handlePlanProgress(ev: JobProgressEvent) {
    if (ev.delta) setPlanDelta((prev) => prev + ev.delta);
    if (ev.stage === "completed") {
      void (async () => {
        try { handlePlanResult((await api.getJob(ev.jobId)) as Job); }
        catch (err) { setError(`fetch plan job: ${String(err)}`); }
      })();
    } else if (
      ev.stage === "failed" || ev.stage.includes("error") ||
      ev.stage === "stream-start-error" || ev.stage === "stream-error"
    ) {
      setError(`plan failed at ${ev.stage}: ${ev.message ?? ""}`);
      setPhase("idle");
      clearPlanStore();
    }
  }

  function handleCodeProgress(ev: JobProgressEvent) {
    if (ev.delta) setCodeDelta((prev) => prev + ev.delta);
    if (ev.stage === "completed") {
      void (async () => {
        try { handleCodeResult((await api.getJob(ev.jobId)) as Job); }
        catch (err) { setError(`fetch code job: ${String(err)}`); }
      })();
    } else if (ev.stage === "failed" || ev.stage.includes("error")) {
      setError(`code failed at ${ev.stage}: ${ev.message ?? ""}`);
      setPhase("plan_done");
      clearCodeStore();
    }
  }

  // 挂载时检查 workflowStore 中是否有未完成的 job
  // job.status type is "failed" | "completed" | "cancelled" — if it's NOT one of
  // those (i.e. the job JSON has no status field yet), treat as still-running
  useEffect(() => {
    if (wfStore.planJobId) {
      void (async () => {
        try {
          const job = (await api.getJob(wfStore.planJobId!)) as Job;
          if (job.status === "completed") {
            handlePlanResult(job);
          } else if (job.status !== "failed" && job.status !== "cancelled") {
            setPhase("planning");
          } else {
            clearPlanStore();
          }
        } catch { clearPlanStore(); }
      })();
    }
    if (wfStore.codeJobId) {
      void (async () => {
        try {
          const job = (await api.getJob(wfStore.codeJobId!)) as Job;
          if (job.status === "completed") {
            handleCodeResult(job);
          } else if (job.status !== "failed" && job.status !== "cancelled") {
            setPhase("generating");
          } else {
            clearCodeStore();
          }
        } catch { clearCodeStore(); }
      })();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function runPlan() {
    if (!requirements.trim()) { setError("requirements 不能为空"); return; }
    setError(null); setPlanDelta(""); setPlanItem(null);
    setCodeDelta(""); setCodeResult(null); setPhase("planning");
    try {
      const ack = (await api.submitSingleAssetPlanJob({
        requirements: requirements.trim(), asset_type: assetType,
      })) as SubmitJobAck;
      planJobIdRef.current = ack.jobId;
      useWorkflowStore.getState().setPlanJobId(ack.jobId);
    } catch (e: unknown) { setError(String(e)); setPhase("idle"); }
  }

  async function runCode() {
    if (!planItem || !project) return;
    setError(null); setCodeDelta(""); setCodeResult(null); setPhase("generating");
    try {
      const description = [planItem.description ?? "", planItem.detailed_description ?? ""]
        .filter(Boolean).join("\n\n");
      const name = planItem.name || planItem.id || "Unnamed";
      const needsImage = planItem.needs_image === true && planItem.type !== "custom_code";
      let ack: SubmitJobAck;
      if (needsImage) {
        ack = (await api.submitAssetGenerateJob({
          asset_request: {
            design_description: description, asset_type: planItem.type || "card",
            asset_name: name, image_paths: [], project_root: project.path,
            name_zhs: planItem.name_zhs ?? "", skip_build: true,
          },
          image_prompt: planItem.image_description?.trim()
            ? planItem.image_description.trim() : description,
          image_size: null,
        })) as SubmitJobAck;
      } else {
        ack = (await api.submitCodeGenerateJob({
          mode: "custom_code",
          request: {
            name, description,
            implementation_notes: planItem.implementation_notes ?? "",
            project_root: project.path, skip_build: true,
          },
        })) as SubmitJobAck;
      }
      codeJobIdRef.current = ack.jobId;
      useWorkflowStore.getState().setCodeJobId(ack.jobId);
    } catch (e: unknown) { setError(String(e)); setPhase("plan_done"); }
  }

  function reset() {
    setPhase("idle"); setError(null);
    setPlanDelta(""); setPlanItem(null);
    setCodeDelta(""); setCodeResult(null);
    clearPlanStore(); clearCodeStore();
  }

  if (!__IS_TAURI__) {
    return <Card eyebrow="workflow · single asset" title="Single Asset Workflow" subtitle="desktop-only" />;
  }

  return (
    <Card
      eyebrow="workflow · single asset"
      title="Single Asset Workflow"
      actions={
        phase !== "idle" ? (
          <Button size="sm" onClick={reset}>Reset</Button>
        ) : undefined
      }
    >
      {error && <div data-testid="single-asset-error"><Notice variant="error" title={`Error: ${error}`} /></div>}

      {phase === "plan_done" && planItem && (
        <div data-testid="single-asset-plan-result">
        <CardSection title="plan 结果">
          <div className="grid gap-2" style={{ fontSize: "12px" }}>
            <div><span style={{ color: "var(--ink-mute)" }}>name:</span> {planItem.name ?? planItem.id}</div>
            {planItem.name_zhs && <div><span style={{ color: "var(--ink-mute)" }}>name_zhs:</span> {planItem.name_zhs}</div>}
            <div><span style={{ color: "var(--ink-mute)" }}>type:</span> {planItem.type}</div>
            <div style={{ color: "var(--ink-mute)" }}>{planItem.description ?? planItem.detailed_description}</div>
            <div>
              <span style={{ fontSize: "11px", background: "var(--rule-soft)", padding: "2px 6px", borderRadius: "3px" }}>
                {planItem.needs_image ? "🖼 needs image" : "📄 code only"}
              </span>
            </div>
          </div>
        </CardSection>
        </div>
      )}

      {phase === "code_done" && codeResult && (
        <div data-testid="single-asset-code-result">
        <CardSection title="code 结果">
          <div className="grid gap-2" style={{ fontSize: "12px" }}>
            <div><span style={{ color: "var(--ink-mute)" }}>csPath:</span> <code>{codeResult.csPath}</code></div>
            {codeResult.artifactCsPath && (
              <div><span style={{ color: "var(--ink-mute)" }}>artifactCsPath:</span> <code>{codeResult.artifactCsPath}</code></div>
            )}
            <div><span style={{ color: "var(--ink-mute)" }}>rawPath:</span> <code>{codeResult.rawPath}</code></div>
            <div><span style={{ color: "var(--ink-mute)" }}>extractedChars:</span> {codeResult.extractedChars}</div>
            {codeResult.pngPath && (
              <div><span style={{ color: "var(--ink-mute)" }}>pngPath:</span> <code>{codeResult.pngPath}</code></div>
            )}
          </div>
        </CardSection>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3">
        <Field label="requirements" hint="描述你想要的 asset">
          <textarea
            value={requirements}
            onChange={(e) => setRequirements(e.target.value)}
            rows={4}
            className="input-mono"
            placeholder="做一个回合开始时获得 3 点格挡的卡牌"
            data-testid="single-asset-requirements"
          />
        </Field>
        <Field label="type" hint="asset 类型">
          <select
            value={assetType}
            onChange={(e) => setAssetType(e.target.value as AssetItemType)}
            className="input-mono"
            data-testid="single-asset-type"
          >
            {ASSET_TYPES.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
        </Field>
      </div>

      {phase === "planning" && (
        <div data-testid="single-asset-planning" className="mt-3" style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
          {planDelta ? (
            <pre className="pre-block mt-1" style={{ maxHeight: 200, overflow: "auto", fontSize: "11.5px" }}>
              {planDelta}
            </pre>
          ) : (
            <span>等待 LLM 首帧…</span>
          )}
        </div>
      )}

      {phase === "generating" && (
        <div data-testid="single-asset-generating" className="mt-3" style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
          {codeDelta ? (
            <pre className="pre-block mt-1" style={{ maxHeight: 200, overflow: "auto", fontSize: "11.5px" }}>
              {codeDelta}
            </pre>
          ) : (
            <span>代码生成中…</span>
          )}
        </div>
      )}

      <div className="mt-4 flex gap-3 flex-wrap">
        <Button
          variant="accent"
          onClick={() => void runPlan()}
          disabled={!requirements.trim() || phase !== "idle" || !project}
          data-testid="single-asset-plan-submit"
        >
          1. Generate Plan
        </Button>
        <Button
          variant="accent"
          onClick={() => void runCode()}
          disabled={!planItem || !project || phase !== "plan_done"}
          data-testid="single-asset-code-submit"
        >
          2. Generate Code
        </Button>
      </div>
    </Card>
  );
}
