// 单资产工作流 MVP：自然语言需求 → single_asset_plan handler 出 PlanItem
// → 用户校对/编辑 → code_generate handler 出 .cs 文件。
//
// 目标：让用户在 UI 里走完一条端到端链路，验证 LLM 第三方代理通路可用。
// 桌面端 only（依赖 Tauri job-progress 事件）。

import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import type {
  AssetItemType,
  Job,
  JobProgressEvent,
  PlanItem,
  ProjectSnapshot,
  SubmitJobAck,
} from "@/services/tauriApi";

type Phase = "idle" | "planning" | "plan_done" | "generating" | "code_done";

const ASSET_TYPES: AssetItemType[] = [
  "card",
  "card_fullscreen",
  "relic",
  "power",
  "character",
  "custom_code",
];

export function SingleAssetWorkflowCard() {
  const [project, setProject] = useState<ProjectSnapshot | null>(null);
  const [requirements, setRequirements] = useState(
    "做一个回合开始时获得 3 点格挡的卡牌",
  );
  const [assetType, setAssetType] = useState<AssetItemType>("card");

  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);

  // Plan 阶段状态
  const [planJobId, setPlanJobId] = useState<string | null>(null);
  const [planDelta, setPlanDelta] = useState("");
  const [planItem, setPlanItem] = useState<PlanItem | null>(null);

  // Code 阶段状态
  const [codeJobId, setCodeJobId] = useState<string | null>(null);
  const [codeDelta, setCodeDelta] = useState("");
  const [codeResult, setCodeResult] = useState<{
    csPath: string;
    rawPath: string;
    extractedChars: number;
  } | null>(null);

  // 监听 job-progress 事件，按 jobId 派发
  const unlistenRef = useRef<(() => void) | null>(null);
  const planJobIdRef = useRef<string | null>(null);
  const codeJobIdRef = useRef<string | null>(null);

  useEffect(() => {
    planJobIdRef.current = planJobId;
  }, [planJobId]);
  useEffect(() => {
    codeJobIdRef.current = codeJobId;
  }, [codeJobId]);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        const snap = await api.currentProject();
        setProject(snap as ProjectSnapshot | null);
      } catch (e: unknown) {
        // 没有 active project 不致命，UI 自己提示。
        console.warn("currentProject:", e);
      }
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const ev = e.payload;
        if (ev.jobId === planJobIdRef.current) {
          handlePlanProgress(ev);
        } else if (ev.jobId === codeJobIdRef.current) {
          handleCodeProgress(ev);
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

  function handlePlanProgress(ev: JobProgressEvent) {
    if (ev.delta) {
      setPlanDelta((prev) => prev + ev.delta);
    }
    if (ev.stage === "completed") {
      void (async () => {
        try {
          const job = (await api.getJob(ev.jobId)) as Job;
          // job.result.item 是 PlanItem
          const result = job.result as { item?: PlanItem } | null;
          if (result?.item) {
            setPlanItem(result.item);
          }
          setPhase("plan_done");
        } catch (err) {
          setError(`fetch plan job: ${String(err)}`);
        }
      })();
    } else if (
      ev.stage.includes("error") ||
      ev.stage === "stream-start-error" ||
      ev.stage === "stream-error"
    ) {
      setError(`plan failed at ${ev.stage}: ${ev.message ?? ""}`);
      setPhase("idle");
    }
  }

  function handleCodeProgress(ev: JobProgressEvent) {
    if (ev.delta) {
      setCodeDelta((prev) => prev + ev.delta);
    }
    if (ev.stage === "completed") {
      void (async () => {
        try {
          const job = (await api.getJob(ev.jobId)) as Job;
          const r = job.result as {
            csPath?: string;
            rawPath?: string;
            extractedChars?: number;
          } | null;
          if (r?.csPath) {
            setCodeResult({
              csPath: r.csPath,
              rawPath: r.rawPath ?? "",
              extractedChars: r.extractedChars ?? 0,
            });
          }
          setPhase("code_done");
        } catch (err) {
          setError(`fetch code job: ${String(err)}`);
        }
      })();
    } else if (ev.stage.includes("error")) {
      setError(`code failed at ${ev.stage}: ${ev.message ?? ""}`);
      setPhase("plan_done");
    }
  }

  async function runPlan() {
    if (!requirements.trim()) {
      setError("requirements 不能为空");
      return;
    }
    setError(null);
    setPlanDelta("");
    setPlanItem(null);
    setCodeDelta("");
    setCodeResult(null);
    setPhase("planning");
    try {
      const ack = (await api.submitSingleAssetPlanJob({
        requirements: requirements.trim(),
        asset_type: assetType,
      })) as SubmitJobAck;
      setPlanJobId(ack.jobId);
    } catch (e: unknown) {
      setError(String(e));
      setPhase("idle");
    }
  }

  async function runCode() {
    if (!planItem || !project) return;
    setError(null);
    setCodeDelta("");
    setCodeResult(null);
    setPhase("generating");
    try {
      // 用 custom_code 模式：用 plan 的 name + description + implementation_notes
      const description = [planItem.description ?? "", planItem.detailed_description ?? ""]
        .filter(Boolean)
        .join("\n\n");
      const ack = (await api.submitCodeGenerateJob({
        mode: "custom_code",
        request: {
          name: planItem.name || planItem.id || "Unnamed",
          description,
          implementation_notes: planItem.implementation_notes ?? "",
          project_root: project.path,
          skip_build: true,
        },
      })) as SubmitJobAck;
      setCodeJobId(ack.jobId);
    } catch (e: unknown) {
      setError(String(e));
      setPhase("plan_done");
    }
  }

  function reset() {
    setPhase("idle");
    setError(null);
    setPlanJobId(null);
    setPlanDelta("");
    setPlanItem(null);
    setCodeJobId(null);
    setCodeDelta("");
    setCodeResult(null);
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Single-Asset Workflow</h2>
        <p className="text-muted text-sm">
          桌面端 only —— Web 模式下需要 Stage 3.6 ats-web platform routes 落地后再启用。
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-medium">Single-Asset Workflow（MVP）</h2>
        {phase !== "idle" && (
          <button
            type="button"
            onClick={reset}
            className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10"
          >
            Reset
          </button>
        )}
      </div>

      {!project && (
        <p className="text-amber-600 text-sm">
          请先在 Project 卡片中"打开"或"新建"一个工程，才能跑代码生成。
        </p>
      )}

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      {/* 阶段 1：输入需求 */}
      <fieldset
        disabled={phase === "planning" || phase === "generating"}
        className="space-y-2"
      >
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Requirements（自然语言）</span>
          <textarea
            value={requirements}
            onChange={(e) => setRequirements(e.target.value)}
            rows={3}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent text-sm"
            placeholder="例：做一个回合开始时获得 3 点格挡的卡牌"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Asset type</span>
          <select
            value={assetType}
            onChange={(e) => setAssetType(e.target.value as AssetItemType)}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent text-sm"
          >
            {ASSET_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          onClick={runPlan}
          disabled={phase === "planning" || phase === "generating"}
          className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
        >
          {phase === "planning" ? "Planning…" : "1. Generate Plan"}
        </button>
      </fieldset>

      {/* 阶段 2：流式 LLM 输出 + PlanItem 结果 */}
      {(phase === "planning" || planDelta) && (
        <div className="space-y-2">
          <p className="text-xs text-muted">
            Plan LLM stream {planJobId && <code className="ml-1">{planJobId.slice(0, 8)}</code>}
          </p>
          <pre className="text-xs p-2 rounded border border-muted/20 max-h-48 overflow-auto whitespace-pre-wrap bg-muted/5">
            {planDelta || (phase === "planning" ? "等待 LLM 首帧…" : "")}
          </pre>
        </div>
      )}

      {planItem && (
        <details open className="rounded border border-emerald-500/30 p-2">
          <summary className="cursor-pointer text-sm font-medium">
            PlanItem ✓ <code className="text-xs text-muted">{planItem.id}</code>
          </summary>
          <dl className="text-xs mt-2 grid grid-cols-[120px_1fr] gap-x-2 gap-y-1">
            <dt className="text-muted">name</dt>
            <dd>
              {planItem.name}
              {planItem.name_zhs && (
                <span className="text-muted"> · {planItem.name_zhs}</span>
              )}
            </dd>
            <dt className="text-muted">type</dt>
            <dd>{planItem.type}</dd>
            <dt className="text-muted">description</dt>
            <dd>{planItem.description}</dd>
            <dt className="text-muted">goal</dt>
            <dd>{planItem.goal}</dd>
            <dt className="text-muted">implementation_notes</dt>
            <dd className="whitespace-pre-wrap">{planItem.implementation_notes}</dd>
            <dt className="text-muted">acceptance_notes</dt>
            <dd className="whitespace-pre-wrap">{planItem.acceptance_notes}</dd>
          </dl>
          <details className="mt-2">
            <summary className="cursor-pointer text-xs text-muted">完整 JSON</summary>
            <pre className="text-xs mt-1 max-h-48 overflow-auto whitespace-pre-wrap">
              {JSON.stringify(planItem, null, 2)}
            </pre>
          </details>
        </details>
      )}

      {/* 阶段 3：代码生成 */}
      {planItem && project && (
        <button
          type="button"
          onClick={runCode}
          disabled={phase === "generating"}
          className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
        >
          {phase === "generating" ? "Generating…" : "2. Generate Code"}
        </button>
      )}

      {(phase === "generating" || codeDelta) && (
        <div className="space-y-2">
          <p className="text-xs text-muted">
            Code LLM stream {codeJobId && <code className="ml-1">{codeJobId.slice(0, 8)}</code>}
          </p>
          <pre className="text-xs p-2 rounded border border-muted/20 max-h-64 overflow-auto whitespace-pre-wrap bg-muted/5">
            {codeDelta || (phase === "generating" ? "等待 LLM 首帧…" : "")}
          </pre>
        </div>
      )}

      {codeResult && (
        <div className="rounded border border-emerald-500/30 p-2 text-xs space-y-1">
          <p className="font-medium text-emerald-600">代码已落盘 ✓</p>
          <p>
            <span className="text-muted">.cs：</span>
            <code className="break-all">{codeResult.csPath}</code>
          </p>
          <p>
            <span className="text-muted">raw.md：</span>
            <code className="break-all">{codeResult.rawPath}</code>
          </p>
          <p className="text-muted">
            提取代码 {codeResult.extractedChars} 字符。在工程目录里查看。
          </p>
        </div>
      )}
    </section>
  );
}
