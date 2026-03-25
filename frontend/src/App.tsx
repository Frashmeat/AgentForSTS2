import { useState, useCallback, useRef, useEffect } from "react";
import { Settings, Swords } from "lucide-react";
import { SettingsPanel } from "./components/SettingsPanel";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "./lib/approvals";
import { WorkflowSocket } from "./lib/ws";
import { cn } from "./lib/utils";
import { BatchGenerationFeatureView } from "./features/batch-generation/view";
import { LogAnalysisFeatureView } from "./features/log-analysis/view";
import { ModEditorFeatureView } from "./features/mod-editor/view";
import { type AssetType, getStageIndex, type Stage } from "./features/single-asset/model";
import { SingleAssetFeatureView } from "./features/single-asset/view";

type AppTab = "single" | "batch" | "edit" | "log";

export default function App() {
  const [activeTab, setActiveTab] = useState<AppTab>("single");
  const [stage, setStage] = useState<Stage>("input");
  const stageRef = useRef<Stage>("input");
  function updateStage(s: Stage) { stageRef.current = s; setStage(s); }

  const [assetType, setAssetType] = useState<AssetType>("relic");
  const [assetName, setAssetName] = useState("");
  const [description, setDescription] = useState("");
  const [projectRoot, setProjectRoot] = useState("");

  const [images, setImages] = useState<string[]>([]);
  const [pendingSlots, setPendingSlots] = useState(0);
  const batchOffsetRef = useRef(0);
  const [promptPreview, setPromptPreview] = useState("");
  const [negativePrompt, setNegativePrompt] = useState("");
  const [promptFallbackWarn, setPromptFallbackWarn] = useState<string | null>(null);
  const [currentPrompt, setCurrentPrompt] = useState("");
  const [showMorePrompt, setShowMorePrompt] = useState(false);

  const [genLog, setGenLog] = useState<string[]>([]);
  const [agentLog, setAgentLog] = useState<string[]>([]);
  const [flowStageCurrent, setFlowStageCurrent] = useState<string | null>(null);
  const [flowStageHistory, setFlowStageHistory] = useState<string[]>([]);
  const [agentStageCurrent, setAgentStageCurrent] = useState<string | null>(null);
  const [agentStageHistory, setAgentStageHistory] = useState<string[]>([]);
  const [approvalSummary, setApprovalSummary] = useState("");
  const [approvalRequests, setApprovalRequests] = useState<ApprovalRequest[]>([]);
  const [approvalBusyActionId, setApprovalBusyActionId] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [errorTrace, setErrorTrace] = useState<string | null>(null);
  const [socket, setSocket] = useState<WorkflowSocket | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [autoMode, setAutoMode] = useState(false);
  const autoModeRef = useRef(false);
  const [imageMode, setImageMode] = useState<"ai" | "upload">("ai");
  const [uploadedImageB64, setUploadedImageB64] = useState<string>("");
  const [uploadedImageName, setUploadedImageName] = useState<string>("");
  const [uploadedImagePreview, setUploadedImagePreview] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);

  // 启动时从 config 读默认项目路径
  useEffect(() => {
    fetch("/api/config").then(r => r.json()).then(cfg => {
      if (cfg?.default_project_root && !projectRoot) {
        setProjectRoot(cfg.default_project_root);
      }
    }).catch(() => {});
  }, []);

  const appendGen   = useCallback((m: string) => setGenLog(p => [...p, m]), []);
  const appendAgent = useCallback((m: string) => setAgentLog(p => [...p, m]), []);
  const pushStage = useCallback((scope: string, message: string) => {
    if (scope === "agent") {
      setAgentStageCurrent(message);
      setAgentStageHistory(prev => prev[prev.length - 1] === message ? prev : [...prev, message]);
      return;
    }
    setFlowStageCurrent(message);
    setFlowStageHistory(prev => prev[prev.length - 1] === message ? prev : [...prev, message]);
  }, []);

  const step = getStageIndex(stage);

  async function startWorkflow() {
    if (!assetName.trim() || !description.trim() || !projectRoot.trim()) return;
    setGenLog([]);
    setAgentLog([]);
    setFlowStageCurrent(null);
    setFlowStageHistory([]);
    setAgentStageCurrent(null);
    setAgentStageHistory([]);
    setApprovalSummary("");
    setApprovalRequests([]);
    setApprovalBusyActionId(null);
    setImages([]);
    setPendingSlots(0);
    batchOffsetRef.current = 0;
    setPromptPreview("");
    setNegativePrompt("");
    setPromptFallbackWarn(null);
    setCurrentPrompt("");
    setShowMorePrompt(false);
    setErrorMsg(null);
    setErrorTrace(null);
    // upload 模式直接跳过生图阶段；ai 模式先进 generating_image
    if (imageMode !== "upload") updateStage("generating_image");

    const ws = new WorkflowSocket();
    setSocket(ws);
    ws.on("stage_update",   (d: any) => pushStage(d.scope, d.message));
    ws.on("progress",       (d: any) => appendGen(`${d.message}`));
    ws.on("agent_stream",   (d: any) => appendAgent(d.chunk));
    ws.on("error",          (d: any) => { setErrorMsg(d.message); setErrorTrace(d.traceback || null); updateStage("error"); });
    ws.on("approval_pending", (d: any) => {
      setApprovalSummary(d.summary || "");
      setApprovalRequests(d.requests || []);
      appendAgent("已生成待审批动作，等待用户审批后继续执行。");
      updateStage("approval_pending");
    });
    ws.on("prompt_preview", (d: any) => {
      if (autoModeRef.current) {
        ws.send({ action: "confirm", prompt: d.prompt, negative_prompt: d.negative_prompt || "" });
        appendGen("自动模式：跳过 prompt 确认");
        return;
      }
      setPromptPreview(d.prompt);
      setCurrentPrompt(d.prompt);
      setNegativePrompt(d.negative_prompt || "");
      setPromptFallbackWarn(d.fallback_warning || null);
      updateStage("confirm_prompt");
    });
    ws.on("image_ready", (d: any) => {
      setImages(prev => {
        const next = [...prev];
        next[batchOffsetRef.current + d.index] = d.image;
        return next;
      });
      setPendingSlots(0);
      setCurrentPrompt(d.prompt);
      setShowMorePrompt(false);
      if (autoModeRef.current) {
        appendGen("自动模式：自动选第 1 张图");
        ws.send({ action: "select", index: 0 });
        updateStage("agent_running");
        return;
      }
      updateStage("pick_image");
    });
    ws.on("done", (d: any) => {
      appendAgent(d.success ? "✓ 构建成功！" : "✗ 构建失败");
      updateStage("done");
    });

    try {
      await ws.waitOpen();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
      updateStage("error");
      return;
    }
    if (imageMode === "upload" && uploadedImageB64) {
      updateStage("agent_running");
      ws.send({ action: "start", asset_type: assetType, asset_name: assetName, description, project_root: projectRoot, provided_image_b64: uploadedImageB64, provided_image_name: uploadedImageName });
    } else {
      ws.send({ action: "start", asset_type: assetType, asset_name: assetName, description, project_root: projectRoot });
    }
  }

  function handleConfirmPrompt() {
    if (!socket) return;
    updateStage("generating_image");
    socket.send({ action: "confirm", prompt: promptPreview, negative_prompt: negativePrompt });
  }

  function handleSelectImage(index: number) {
    if (!socket) return;
    updateStage("agent_running");
    socket.send({ action: "select", index });
  }

  function handleGenerateMore() {
    if (!socket) return;
    batchOffsetRef.current = images.length;
    setPendingSlots(1);
    setShowMorePrompt(false);
    socket.send({ action: "generate_more", prompt: currentPrompt, negative_prompt: negativePrompt || undefined });
  }

  function handleImageFile(file: File) {
    setUploadedImageName(file.name);
    const reader = new FileReader();
    reader.onload = ev => {
      const dataUrl = ev.target?.result as string;
      setUploadedImagePreview(dataUrl);
      // 去掉 "data:image/png;base64," 前缀，只保留纯 base64
      setUploadedImageB64(dataUrl.split(",")[1] ?? "");
    };
    reader.readAsDataURL(file);
  }

  function reset() {
    socket?.close();
    setSocket(null);
    updateStage("input");
    setUploadedImageB64("");
    setUploadedImageName("");
    setUploadedImagePreview(null);
    setImages([]);
    setPendingSlots(0);
    batchOffsetRef.current = 0;
    setGenLog([]);
    setAgentLog([]);
    setFlowStageCurrent(null);
    setFlowStageHistory([]);
    setAgentStageCurrent(null);
    setAgentStageHistory([]);
    setApprovalSummary("");
    setApprovalRequests([]);
    setApprovalBusyActionId(null);
    setPromptPreview("");
    setNegativePrompt("");
    setPromptFallbackWarn(null);
    setCurrentPrompt("");
    setShowMorePrompt(false);
    setErrorMsg(null);
    setErrorTrace(null);
  }

  // 判断错误发生在哪个阶段，用于在对应步骤内显示
  const errorInStep2 = stage === "error" && step <= 2;
  const errorInStep3 = stage === "error" && step > 2;

  async function handleApprovalAction(
    actionId: string,
    action: (id: string) => Promise<ApprovalRequest>,
  ) {
    setApprovalBusyActionId(actionId);
    try {
      const updated = await action(actionId);
      setApprovalRequests(prev => prev.map(req => req.action_id === actionId ? updated : req));
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      updateStage("error");
    } finally {
      setApprovalBusyActionId(null);
    }
  }

  return (
    <div className="min-h-screen bg-slate-50 text-slate-800">
      {/* Header */}
      <header className="sticky top-0 z-10 border-b border-slate-200 px-6 py-3 flex items-center justify-between bg-white/80 backdrop-blur-sm shadow-sm">
        <div className="flex items-center gap-2">
          <Swords className="text-amber-600" size={22} />
          <span className="font-bold tracking-wide text-amber-600 text-lg">AgentTheSpire</span>
        </div>
        <button onClick={() => setSettingsOpen(true)} className="flex items-center gap-1.5 py-1.5 px-3 rounded-lg bg-slate-100 hover:bg-amber-50 hover:text-amber-700 text-slate-500 hover:border-amber-300 border border-transparent transition-colors text-sm font-medium">
          <Settings size={14} />
          设置
        </button>
      </header>

      {/* Tab 切换 */}
      <div className="px-6 pt-4 flex gap-1 border-b border-slate-200 bg-white">
        <button
          onClick={() => setActiveTab("single")}
          className={cn(
            "px-4 py-2 text-sm font-medium rounded-t-lg border-b-2 transition-colors",
            activeTab === "single"
              ? "border-amber-500 text-amber-600 bg-amber-50"
              : "border-transparent text-slate-400 hover:text-slate-600"
          )}
        >
          单资产
        </button>
        <button
          onClick={() => setActiveTab("batch")}
          className={cn(
            "px-4 py-2 text-sm font-medium rounded-t-lg border-b-2 transition-colors",
            activeTab === "batch"
              ? "border-amber-500 text-amber-600 bg-amber-50"
              : "border-transparent text-slate-400 hover:text-slate-600"
          )}
        >
          Mod 规划
        </button>
        <button
          onClick={() => setActiveTab("edit")}
          className={cn(
            "px-4 py-2 text-sm font-medium rounded-t-lg border-b-2 transition-colors",
            activeTab === "edit"
              ? "border-amber-500 text-amber-600 bg-amber-50"
              : "border-transparent text-slate-400 hover:text-slate-600"
          )}
        >
          修改 Mod
        </button>
        <button
          onClick={() => setActiveTab("log")}
          className={cn(
            "px-4 py-2 text-sm font-medium rounded-t-lg border-b-2 transition-colors",
            activeTab === "log"
              ? "border-amber-500 text-amber-600 bg-amber-50"
              : "border-transparent text-slate-400 hover:text-slate-600"
          )}
        >
          崩溃分析
        </button>
      </div>

      {activeTab === "batch" && (
        <div className="px-6 py-6">
          <BatchGenerationFeatureView />
        </div>
      )}

      {activeTab === "edit" && (
        <div className="px-6 py-6">
          <ModEditorFeatureView />
        </div>
      )}

      {activeTab === "log" && (
        <div className="px-6 py-6">
          <LogAnalysisFeatureView />
        </div>
      )}

      {activeTab === "single" && (
        <SingleAssetFeatureView
          step={step}
          stage={stage}
          assetType={assetType}
          assetName={assetName}
          description={description}
          projectRoot={projectRoot}
          images={images}
          pendingSlots={pendingSlots}
          promptPreview={promptPreview}
          negativePrompt={negativePrompt}
          promptFallbackWarn={promptFallbackWarn}
          currentPrompt={currentPrompt}
          showMorePrompt={showMorePrompt}
          genLog={genLog}
          agentLog={agentLog}
          flowStageCurrent={flowStageCurrent}
          flowStageHistory={flowStageHistory}
          agentStageCurrent={agentStageCurrent}
          agentStageHistory={agentStageHistory}
          approvalSummary={approvalSummary}
          approvalRequests={approvalRequests}
          approvalBusyActionId={approvalBusyActionId}
          errorMsg={errorMsg}
          errorTrace={errorTrace}
          autoMode={autoMode}
          imageMode={imageMode}
          uploadedImageB64={uploadedImageB64}
          uploadedImageName={uploadedImageName}
          uploadedImagePreview={uploadedImagePreview}
          dragOver={dragOver}
          onAssetTypeChange={setAssetType}
          onAssetNameChange={setAssetName}
          onDescriptionChange={setDescription}
          onProjectRootChange={setProjectRoot}
          onApplyPreset={(preset) => {
            setAssetType(preset.assetType);
            setAssetName(preset.assetName);
            setDescription(preset.description);
          }}
          onStartWorkflow={startWorkflow}
          onReset={reset}
          onImageModeChange={setImageMode}
          onAutoModeToggle={() => {
            const next = !autoMode;
            setAutoMode(next);
            autoModeRef.current = next;
          }}
          onPromptPreviewChange={setPromptPreview}
          onNegativePromptChange={setNegativePrompt}
          onConfirmPrompt={handleConfirmPrompt}
          onSelectImage={handleSelectImage}
          onGenerateMore={handleGenerateMore}
          onCurrentPromptChange={setCurrentPrompt}
          onToggleShowMorePrompt={() => setShowMorePrompt((value) => !value)}
          onHandleImageFile={handleImageFile}
          onDragOverChange={setDragOver}
          onApprove={(actionId) => {
            void handleApprovalAction(actionId, approveApproval);
          }}
          onReject={(actionId) => {
            void handleApprovalAction(actionId, (id) => rejectApproval(id));
          }}
          onExecute={(actionId) => {
            void handleApprovalAction(actionId, executeApproval);
          }}
          onProceedApproval={() => {
            socket?.send({ action: "approve_all" });
          }}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      )}

      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
