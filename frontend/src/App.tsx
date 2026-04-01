import { useState, useRef, useEffect, useReducer } from "react";
import { Settings, Swords } from "lucide-react";
import { SettingsPanel } from "./components/SettingsPanel";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "./shared/api/approvals";
import { createSingleAssetSocket, type SingleAssetSocket } from "./lib/single_asset_ws";
import { cn } from "./lib/utils";
import { BatchGenerationFeatureView } from "./features/batch-generation/view";
import { LogAnalysisFeatureView } from "./features/log-analysis/view";
import { ModEditorFeatureView } from "./features/mod-editor/view";
import { type AssetType, getStageIndex, type Stage } from "./features/single-asset/model";
import { createInitialSingleAssetWorkflowState, singleAssetWorkflowReducer } from "./features/single-asset/state";
import {
  clearSingleAssetSnapshot,
  hasSingleAssetRecoveryContext,
  loadSingleAssetSnapshot,
  refreshRecoveredSingleAssetApprovals,
  saveSingleAssetSnapshot,
} from "./features/single-asset/recovery";
import { SingleAssetFeatureView } from "./features/single-asset/view";
import { loadAppConfig, resolveMigrationFlags, type WorkflowMigrationFlags } from "./shared/api/config";

type AppTab = "single" | "batch" | "edit" | "log";

export default function App() {
  const [activeTab, setActiveTab] = useState<AppTab>("single");
  const [initialSingleAssetSnapshot] = useState(() => loadSingleAssetSnapshot());
  const [assetType, setAssetType] = useState<AssetType>(() => initialSingleAssetSnapshot?.assetType ?? "relic");
  const [assetName, setAssetName] = useState(() => initialSingleAssetSnapshot?.assetName ?? "");
  const [description, setDescription] = useState(() => initialSingleAssetSnapshot?.description ?? "");
  const [projectRoot, setProjectRoot] = useState(() => initialSingleAssetSnapshot?.projectRoot ?? "");

  const [workflowState, dispatchWorkflow] = useReducer(
    singleAssetWorkflowReducer,
    undefined,
    () => initialSingleAssetSnapshot?.workflowState ?? createInitialSingleAssetWorkflowState(),
  );
  const batchOffsetRef = useRef(0);
  const [socket, setSocket] = useState<SingleAssetSocket | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [autoMode, setAutoMode] = useState(() => initialSingleAssetSnapshot?.autoMode ?? false);
  const autoModeRef = useRef(false);
  const [imageMode, setImageMode] = useState<"ai" | "upload">(() => initialSingleAssetSnapshot?.imageMode ?? "ai");
  const [uploadedImageB64, setUploadedImageB64] = useState<string>(() => initialSingleAssetSnapshot?.uploadedImageB64 ?? "");
  const [uploadedImageName, setUploadedImageName] = useState<string>(() => initialSingleAssetSnapshot?.uploadedImageName ?? "");
  const [uploadedImagePreview, setUploadedImagePreview] = useState<string | null>(() => initialSingleAssetSnapshot?.uploadedImagePreview ?? null);
  const [dragOver, setDragOver] = useState(false);
  const [migrationFlags, setMigrationFlags] = useState<WorkflowMigrationFlags>(() => resolveMigrationFlags(undefined));
  const [restoredSnapshotMode, setRestoredSnapshotMode] = useState(() => initialSingleAssetSnapshot !== null);
  const [restoredApprovalRefreshPending, setRestoredApprovalRefreshPending] = useState(
    () => initialSingleAssetSnapshot?.workflowState.stage === "approval_pending",
  );

  // 启动时从 config 读默认项目路径
  useEffect(() => {
    autoModeRef.current = autoMode;
  }, [autoMode]);

  useEffect(() => {
    loadAppConfig()
      .then((config) => {
        setMigrationFlags(resolveMigrationFlags(config));
        if (config?.default_project_root) {
          setProjectRoot((current) => current || String(config.default_project_root));
        }
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const snapshot = {
      assetType,
      assetName,
      description,
      projectRoot,
      imageMode,
      autoMode,
      uploadedImageB64,
      uploadedImageName,
      uploadedImagePreview,
      workflowState,
    };
    if (!hasSingleAssetRecoveryContext(snapshot)) {
      clearSingleAssetSnapshot();
      return;
    }
    saveSingleAssetSnapshot(localStorage, snapshot);
  }, [
    assetType,
    assetName,
    autoMode,
    description,
    imageMode,
    projectRoot,
    uploadedImageB64,
    uploadedImageName,
    uploadedImagePreview,
    workflowState,
  ]);

  useEffect(() => {
    if (!restoredApprovalRefreshPending) {
      return;
    }
    let cancelled = false;
    void refreshRecoveredSingleAssetApprovals({
      assetType,
      assetName,
      description,
      projectRoot,
      imageMode,
      autoMode,
      uploadedImageB64,
      uploadedImageName,
      uploadedImagePreview,
      workflowState,
    }).then((refreshed) => {
      if (cancelled) {
        return;
      }
      dispatchWorkflow({
        type: "approval_requests_updated",
        requests: refreshed.workflowState.approvalRequests,
      });
      setRestoredApprovalRefreshPending(false);
    });

    return () => {
      cancelled = true;
    };
  }, [
    assetName,
    assetType,
    autoMode,
    description,
    imageMode,
    projectRoot,
    restoredApprovalRefreshPending,
    uploadedImageB64,
    uploadedImageName,
    uploadedImagePreview,
    workflowState,
  ]);

  const step = getStageIndex(workflowState.stage);

  async function startWorkflow() {
    if (!assetName.trim() || !description.trim() || !projectRoot.trim()) return;
    setRestoredSnapshotMode(false);
    setRestoredApprovalRefreshPending(false);
    batchOffsetRef.current = 0;
    dispatchWorkflow({ type: "workflow_started", imageMode });

    const ws = createSingleAssetSocket(migrationFlags);
    setSocket(ws);
    ws.on("stage_update",   (d: any) => {
      dispatchWorkflow({
        type: d.scope === "agent" ? "agent_stage_pushed" : "flow_stage_pushed",
        message: d.message,
      });
    });
    ws.on("progress",       (d: any) => dispatchWorkflow({ type: "gen_log_appended", message: `${d.message}` }));
    ws.on("agent_stream",   (d: any) => dispatchWorkflow({ type: "agent_log_appended", message: d.chunk }));
    ws.on("error",          (d: any) => {
      dispatchWorkflow({ type: "workflow_failed", message: d.message, traceback: d.traceback || null });
    });
    ws.on("approval_pending", (d: any) => {
      dispatchWorkflow({
        type: "approval_pending_received",
        summary: d.summary || "",
        requests: d.requests || [],
      });
    });
    ws.on("prompt_preview", (d: any) => {
      if (autoModeRef.current) {
        ws.send({ action: "confirm", prompt: d.prompt, negative_prompt: d.negative_prompt || "" });
        dispatchWorkflow({ type: "gen_log_appended", message: "自动模式：跳过 prompt 确认" });
        return;
      }
      dispatchWorkflow({
        type: "prompt_preview_received",
        prompt: d.prompt,
        negativePrompt: d.negative_prompt || "",
        fallbackWarning: d.fallback_warning || null,
      });
    });
    ws.on("image_ready", (d: any) => {
      dispatchWorkflow({
        type: "image_ready_received",
        index: d.index,
        image: d.image,
        prompt: d.prompt,
        batchOffset: batchOffsetRef.current,
      });
      if (autoModeRef.current) {
        dispatchWorkflow({ type: "gen_log_appended", message: "自动模式：自动选第 1 张图" });
        ws.send({ action: "select", index: 0 });
        dispatchWorkflow({ type: "stage_changed", stage: "agent_running" });
      }
    });
    ws.on("done", (d: any) => {
      dispatchWorkflow({ type: "agent_log_appended", message: d.success ? "✓ 构建成功！" : "✗ 构建失败" });
      dispatchWorkflow({ type: "stage_changed", stage: "done" });
    });

    try {
      await ws.waitOpen();
    } catch (err) {
      dispatchWorkflow({
        type: "workflow_failed",
        message: err instanceof Error ? err.message : String(err),
        traceback: null,
      });
      return;
    }
    if (imageMode === "upload" && uploadedImageB64) {
      dispatchWorkflow({ type: "stage_changed", stage: "agent_running" });
      ws.send({ action: "start", asset_type: assetType, asset_name: assetName, description, project_root: projectRoot, provided_image_b64: uploadedImageB64, provided_image_name: uploadedImageName });
    } else {
      ws.send({ action: "start", asset_type: assetType, asset_name: assetName, description, project_root: projectRoot });
    }
  }

  function handleConfirmPrompt() {
    if (!socket) return;
    dispatchWorkflow({ type: "prompt_confirmed" });
    socket.send({ action: "confirm", prompt: workflowState.promptPreview, negative_prompt: workflowState.negativePrompt });
  }

  function handleSelectImage(index: number) {
    if (!socket) return;
    dispatchWorkflow({ type: "image_selected" });
    socket.send({ action: "select", index });
  }

  function handleGenerateMore() {
    if (!socket) return;
    batchOffsetRef.current = workflowState.images.length;
    dispatchWorkflow({ type: "generate_more_requested", batchOffset: batchOffsetRef.current });
    socket.send({ action: "generate_more", prompt: workflowState.currentPrompt, negative_prompt: workflowState.negativePrompt || undefined });
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
    clearSingleAssetSnapshot();
    setRestoredSnapshotMode(false);
    setRestoredApprovalRefreshPending(false);
    setUploadedImageB64("");
    setUploadedImageName("");
    setUploadedImagePreview(null);
    batchOffsetRef.current = 0;
    dispatchWorkflow({ type: "workflow_reset" });
  }

  async function handleApprovalAction(
    actionId: string,
    action: (id: string) => Promise<ApprovalRequest>,
  ) {
    dispatchWorkflow({ type: "approval_busy_set", actionId });
    try {
      const updated = await action(actionId);
      dispatchWorkflow({
        type: "approval_requests_updated",
        requests: workflowState.approvalRequests.map(req => req.action_id === actionId ? updated : req),
      });
    } catch (error) {
      dispatchWorkflow({
        type: "workflow_failed",
        message: error instanceof Error ? error.message : String(error),
        traceback: null,
      });
    } finally {
      dispatchWorkflow({ type: "approval_busy_set", actionId: null });
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
          stage={workflowState.stage}
          assetType={assetType}
          assetName={assetName}
          description={description}
          projectRoot={projectRoot}
          images={workflowState.images}
          pendingSlots={workflowState.pendingSlots}
          promptPreview={workflowState.promptPreview}
          negativePrompt={workflowState.negativePrompt}
          promptFallbackWarn={workflowState.promptFallbackWarn}
          currentPrompt={workflowState.currentPrompt}
          showMorePrompt={workflowState.showMorePrompt}
          genLog={workflowState.genLog}
          agentLog={workflowState.agentLog}
          flowStageCurrent={workflowState.flowStageCurrent}
          flowStageHistory={workflowState.flowStageHistory}
          agentStageCurrent={workflowState.agentStageCurrent}
          agentStageHistory={workflowState.agentStageHistory}
          approvalSummary={workflowState.approvalSummary}
          approvalRequests={workflowState.approvalRequests}
          approvalBusyActionId={workflowState.approvalBusyActionId}
          errorMsg={workflowState.errorMsg}
          errorTrace={workflowState.errorTrace}
          autoMode={autoMode}
          imageMode={imageMode}
          uploadedImageB64={uploadedImageB64}
          uploadedImageName={uploadedImageName}
          uploadedImagePreview={uploadedImagePreview}
          dragOver={dragOver}
          hasLiveSession={Boolean(socket)}
          showRecoveredNotice={restoredSnapshotMode && !socket && workflowState.stage !== "input"}
          onRestartWorkflow={() => {
            void startWorkflow();
          }}
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
          onPromptPreviewChange={(value) => dispatchWorkflow({ type: "prompt_preview_changed", value })}
          onNegativePromptChange={(value) => dispatchWorkflow({ type: "negative_prompt_changed", value })}
          onConfirmPrompt={handleConfirmPrompt}
          onSelectImage={handleSelectImage}
          onGenerateMore={handleGenerateMore}
          onCurrentPromptChange={(value) => dispatchWorkflow({ type: "current_prompt_changed", value })}
          onToggleShowMorePrompt={() => dispatchWorkflow({ type: "show_more_prompt_toggled" })}
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
