import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import { useRunProgress } from "@/hooks/useRunProgress";
import { useProjectStore } from "@/stores/project";
import { useWorkflowStore } from "@/stores/workflow";
import { Button, Card, CardSection, Field, Notice } from "@/components/ui";
import type {
  AssetItemType,
  RunRecord,
  RunProgressEvent,
  PlanItem,
  SubmitRunAck,
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
    artifactManifestRef: string;
    manifestSha256: string;
    artifactId: string;
    entityName: string;
  } | null>(null);

  const planRunIdRef = useRef<string | null>(null);
  const codeRunIdRef = useRef<string | null>(null);

  planRunIdRef.current = wfStore.planRunId;
  codeRunIdRef.current = wfStore.codeRunId;

  useRunProgress(planRunIdRef, handlePlanProgress);
  useRunProgress(codeRunIdRef, handleCodeProgress);

  function clearPlanStore() { useWorkflowStore.getState().setPlanRunId(null); }
  function clearCodeStore() { useWorkflowStore.getState().setCodeRunId(null); }

  function handlePlanResult(run: RunRecord) {
    if (run.result?.kind !== "plan") {
      setError("plan run returned an incompatible result");
      clearPlanStore();
      return;
    }
    setPlanItem(run.result.item);
    setPhase("plan_done");
    clearPlanStore();
  }

  function handleCodeResult(run: RunRecord) {
    if (run.result?.kind !== "artifact_production") {
      setError("code run returned an incompatible result");
      clearCodeStore();
      return;
    }
    setCodeResult(run.result);
    setPhase("code_done");
    clearCodeStore();
  }

  function handlePlanProgress(ev: RunProgressEvent) {
    if (ev.delta) setPlanDelta((prev) => prev + ev.delta);
    if (ev.stage === "completed") {
      void (async () => {
        try { handlePlanResult((await api.getRun(ev.runId)) as RunRecord); }
        catch (err) { setError(`fetch plan run: ${String(err)}`); }
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

  function handleCodeProgress(ev: RunProgressEvent) {
    if (ev.delta) setCodeDelta((prev) => prev + ev.delta);
    if (ev.stage === "completed") {
      void (async () => {
        try { handleCodeResult((await api.getRun(ev.runId)) as RunRecord); }
        catch (err) { setError(`fetch code run: ${String(err)}`); }
      })();
    } else if (ev.stage === "failed" || ev.stage.includes("error")) {
      setError(`code failed at ${ev.stage}: ${ev.message ?? ""}`);
      setPhase("plan_done");
      clearCodeStore();
    }
  }

  // 挂载时检查 workflowStore 中是否有未完成的 run
  useEffect(() => {
    const pendingPlanRunId = wfStore.planRunId;
    if (pendingPlanRunId) {
      void (async () => {
        try {
          const run = (await api.getRun(pendingPlanRunId)) as RunRecord;
          if (run.status === "succeeded") {
            handlePlanResult(run);
          } else if (run.status !== "failed" && run.status !== "cancelled") {
            setPhase("planning");
          } else {
            clearPlanStore();
          }
        } catch { clearPlanStore(); }
      })();
    }
    const pendingCodeRunId = wfStore.codeRunId;
    if (pendingCodeRunId) {
      void (async () => {
        try {
          const run = (await api.getRun(pendingCodeRunId)) as RunRecord;
          if (run.status === "succeeded") {
            handleCodeResult(run);
          } else if (run.status !== "failed" && run.status !== "cancelled") {
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
      const ack = (await api.submitSingleAssetPlanRun({
        requirements: requirements.trim(), asset_type: assetType,
      })) as SubmitRunAck;
      planRunIdRef.current = ack.runId;
      useWorkflowStore.getState().setPlanRunId(ack.runId);
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
      let ack: SubmitRunAck;
      if (needsImage) {
        ack = (await api.submitAssetGenerateRun({
          asset_request: {
            design_description: description, asset_type: planItem.type || "card",
            asset_name: name, image_paths: [], project_root: project.path,
            name_zhs: planItem.name_zhs ?? "", skip_build: true,
          },
          image_prompt: planItem.image_description?.trim()
            ? planItem.image_description.trim() : description,
          image_size: null,
        })) as SubmitRunAck;
      } else {
        ack = (await api.submitCodeGenerateRun({
          mode: "custom_code",
          request: {
            name, description,
            implementation_notes: planItem.implementation_notes ?? "",
            project_root: project.path, skip_build: true,
          },
        })) as SubmitRunAck;
      }
      codeRunIdRef.current = ack.runId;
      useWorkflowStore.getState().setCodeRunId(ack.runId);
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
            <div><span style={{ color: "var(--ink-mute)" }}>entity:</span> {codeResult.entityName}</div>
            <div><span style={{ color: "var(--ink-mute)" }}>artifact:</span> <code>{codeResult.artifactId}</code></div>
            <div><span style={{ color: "var(--ink-mute)" }}>manifest:</span> <code>{codeResult.artifactManifestRef}</code></div>
            <div><span style={{ color: "var(--ink-mute)" }}>sha256:</span> <code>{codeResult.manifestSha256}</code></div>
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
