// 单资产工作流 MVP：自然语言需求 → single_asset_plan handler 出 PlanItem
// → 用户校对/编辑 → code_generate handler 出 .cs 文件。
//
// 目标：让用户在 UI 里走完一条端到端链路，验证 LLM 第三方代理通路可用。
// 桌面端 only（依赖 Tauri job-progress 事件）。

import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Field,
  KV,
  KVList,
  Notice,
} from "@/components/ui";
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

  const [planJobId, setPlanJobId] = useState<string | null>(null);
  const [planDelta, setPlanDelta] = useState("");
  const [planItem, setPlanItem] = useState<PlanItem | null>(null);

  const [codeJobId, setCodeJobId] = useState<string | null>(null);
  const [codeDelta, setCodeDelta] = useState("");
  const [codeResult, setCodeResult] = useState<{
    csPath: string;
    artifactCsPath?: string | null;
    rawPath: string;
    extractedChars: number;
    pngPath?: string | null;
  } | null>(null);

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
            artifactCsPath?: string | null;
            rawPath?: string;
            extractedChars?: number;
            pngPath?: string | null;
          } | null;
          if (r?.csPath) {
            setCodeResult({
              csPath: r.csPath,
              artifactCsPath: r.artifactCsPath ?? null,
              rawPath: r.rawPath ?? "",
              extractedChars: r.extractedChars ?? 0,
              pngPath: r.pngPath ?? null,
            });
          }
          setPhase("code_done");
        } catch (err) {
          setError(`fetch code job: ${String(err)}`);
        }
      })();
    } else if (ev.stage === "failed" || ev.stage.includes("error")) {
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
      const description = [
        planItem.description ?? "",
        planItem.detailed_description ?? "",
      ]
        .filter(Boolean)
        .join("\n\n");
      const name = planItem.name || planItem.id || "Unnamed";
      const needsImage =
        planItem.needs_image === true && planItem.type !== "custom_code";
      let ack: SubmitJobAck;
      if (needsImage) {
        ack = (await api.submitAssetGenerateJob({
          asset_request: {
            design_description: description,
            asset_type: planItem.type || "card",
            asset_name: name,
            image_paths: [],
            project_root: project.path,
            name_zhs: planItem.name_zhs ?? "",
            skip_build: true,
          },
          image_prompt: planItem.image_description?.trim()
            ? planItem.image_description.trim()
            : description,
          image_size: null,
        })) as SubmitJobAck;
      } else {
        ack = (await api.submitCodeGenerateJob({
          mode: "custom_code",
          request: {
            name,
            description,
            implementation_notes: planItem.implementation_notes ?? "",
            project_root: project.path,
            skip_build: true,
          },
        })) as SubmitJobAck;
      }
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
      <Card
        eyebrow="workflow · single asset"
        title="Single-Asset Workflow"
        subtitle="桌面端 only —— Web 模式下需要 Stage 3.6 ats-web platform routes 落地后再启用。"
      />
    );
  }

  return (
    <Card
      eyebrow="workflow · single asset · mvp"
      title="Single-Asset Workflow"
      actions={
        phase !== "idle" && (
          <Button size="sm" onClick={reset}>
            Reset
          </Button>
        )
      }
    >
      <div className="space-y-3">
        {!project && (
          <Notice
            variant="warn"
            title="先打开或新建一个工程"
          >
            请在 Project 卡片中"打开"或"新建"一个工程，才能跑代码生成。
          </Notice>
        )}

        {error && <Notice variant="error" title={`Error: ${error}`} />}

        <fieldset
          disabled={phase === "planning" || phase === "generating"}
          className="space-y-2 border-0 p-0"
        >
          <Field label="requirements（自然语言）">
            <textarea
              value={requirements}
              onChange={(e) => setRequirements(e.target.value)}
              rows={3}
              placeholder="例：做一个回合开始时获得 3 点格挡的卡牌"
            />
          </Field>
          <Field label="asset type">
            <select
              value={assetType}
              onChange={(e) => setAssetType(e.target.value as AssetItemType)}
            >
              {ASSET_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </Field>
          <Button
            variant="accent"
            onClick={runPlan}
            disabled={phase === "planning" || phase === "generating"}
          >
            {phase === "planning" ? "Planning…" : "1. Generate plan"}
          </Button>
        </fieldset>

        {(phase === "planning" || planDelta) && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="eyebrow-label">Plan LLM stream</span>
              {planJobId && <code style={{ fontSize: "11px" }}>{planJobId.slice(0, 8)}</code>}
              {phase === "planning" && <Badge variant="running">streaming</Badge>}
            </div>
            <pre className="pre-block pre-block-stream max-h-48">
              {planDelta || "等待 LLM 首帧…"}
            </pre>
          </div>
        )}

        {planItem && (
          <div
            className="p-3"
            style={{
              background: "rgba(77, 122, 106, 0.05)",
              border: "1px solid rgba(77, 122, 106, 0.35)",
              borderRadius: "4px",
            }}
          >
            <div className="flex items-center gap-2 mb-3 flex-wrap">
              <Badge variant="ok">PlanItem</Badge>
              <code style={{ fontSize: "11.5px" }}>{planItem.id}</code>
            </div>
            <KVList variant="narrow">
              <KV k="name">
                {planItem.name}
                {planItem.name_zhs && (
                  <span style={{ color: "var(--ink-mute)" }}>
                    {" "} · {planItem.name_zhs}
                  </span>
                )}
              </KV>
              <KV k="type">{planItem.type}</KV>
              <KV k="description">{planItem.description}</KV>
              <KV k="goal">{planItem.goal}</KV>
              <KV k="impl notes">
                <span className="whitespace-pre-wrap">{planItem.implementation_notes}</span>
              </KV>
              <KV k="acceptance">
                <span className="whitespace-pre-wrap">{planItem.acceptance_notes}</span>
              </KV>
            </KVList>
            <details className="mt-3">
              <summary
                className="cursor-pointer"
                style={{ fontSize: "11px", color: "var(--ink-mute)" }}
              >
                完整 JSON
              </summary>
              <pre className="pre-block mt-2 max-h-48">
                {JSON.stringify(planItem, null, 2)}
              </pre>
            </details>
          </div>
        )}

        {planItem && project && (
          <Button
            variant="accent"
            onClick={runCode}
            disabled={phase === "generating"}
          >
            {phase === "generating" ? "Generating…" : "2. Generate code"}
          </Button>
        )}

        {(phase === "generating" || codeDelta) && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="eyebrow-label">Code LLM stream</span>
              {codeJobId && <code style={{ fontSize: "11px" }}>{codeJobId.slice(0, 8)}</code>}
              {phase === "generating" && <Badge variant="running">streaming</Badge>}
            </div>
            <pre className="pre-block pre-block-stream max-h-64">
              {codeDelta || "等待 LLM 首帧…"}
            </pre>
          </div>
        )}

        {codeResult && (
          <Notice variant="ok" title="代码已落盘 ✓">
            <KVList variant="narrow">
              <KV k=".cs">
                <code className="break-all">{codeResult.csPath}</code>
              </KV>
              {codeResult.artifactCsPath && (
                <KV k="artifact copy">
                  <code className="break-all">{codeResult.artifactCsPath}</code>
                </KV>
              )}
              {codeResult.pngPath && (
                <KV k=".png">
                  <code className="break-all">{codeResult.pngPath}</code>
                </KV>
              )}
              <KV k="raw.md">
                <code className="break-all">{codeResult.rawPath}</code>
              </KV>
              <KV k="extracted">{codeResult.extractedChars} 字符</KV>
            </KVList>
          </Notice>
        )}
      </div>
    </Card>
  );
}
