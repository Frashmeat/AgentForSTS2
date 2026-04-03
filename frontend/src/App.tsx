import { useState, useRef, useEffect, useReducer } from "react";
import { Settings, Swords } from "lucide-react";
import { SettingsPanel } from "./components/SettingsPanel";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "./shared/api/index.ts";
import { type SingleAssetSocket } from "./lib/single_asset_ws";
import { cn } from "./lib/utils";
import { BatchGenerationFeatureView } from "./features/batch-generation/view";
import { LogAnalysisFeatureView } from "./features/log-analysis/view";
import { ModEditorFeatureView } from "./features/mod-editor/view";
import { type AssetType, getStageIndex, type Stage } from "./features/single-asset/model";
import { createSingleAssetWorkflowController } from "./features/single-asset/controller.ts";
import { createInitialSingleAssetWorkflowState, singleAssetWorkflowReducer } from "./features/single-asset/state";
import {
  clearSingleAssetSnapshot,
  hasSingleAssetRecoveryContext,
  loadSingleAssetSnapshot,
  refreshRecoveredSingleAssetApprovals,
  saveSingleAssetSnapshot,
} from "./features/single-asset/recovery";
import { SingleAssetFeatureView } from "./features/single-asset/view";
import { resolveMigrationFlags, type WorkflowMigrationFlags } from "./shared/api/index.ts";
import { runApprovalAction } from "./shared/approvalAction.ts";
import { useDefaultProjectRoot } from "./shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "./shared/useProjectCreation.ts";

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
  const {
    projectCreateBusy,
    projectCreateMessage,
    projectCreateError,
    clearProjectCreationFeedback,
    resetProjectCreationState,
    createProjectAtRoot,
  } = useProjectCreation({
    onProjectCreated: setProjectRoot,
  });

  // 启动时从 config 读默认项目路径
  useEffect(() => {
    autoModeRef.current = autoMode;
  }, [autoMode]);

  useDefaultProjectRoot({
    setProjectRoot,
    onConfigLoaded(config) {
      setMigrationFlags(resolveMigrationFlags(config));
    },
  });

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

  const singleAssetWorkflowController = createSingleAssetWorkflowController({
    closeSocket() {
      socket?.close();
    },
    setSocket(nextSocket) {
      setSocket(nextSocket);
    },
    getSocket() {
      return socket;
    },
    clearProjectCreationFeedback,
    setRestoredSnapshotMode,
    setRestoredApprovalRefreshPending,
    dispatchWorkflow,
    reportWorkflowError(message, traceback) {
      dispatchWorkflow({
        type: "workflow_failed",
        message,
        traceback,
      });
    },
  });

  async function startWorkflow() {
    await singleAssetWorkflowController.start({
      assetType,
      assetName,
      description,
      projectRoot,
      imageMode,
      uploadedImageB64,
      uploadedImageName,
      autoMode: autoModeRef.current,
      migrationFlags,
    });
  }

  function handleConfirmPrompt() {
    singleAssetWorkflowController.confirmPrompt(workflowState.promptPreview, workflowState.negativePrompt);
  }

  function handleSelectImage(index: number) {
    singleAssetWorkflowController.selectImage(index);
  }

  function handleGenerateMore() {
    batchOffsetRef.current = workflowState.images.length;
    singleAssetWorkflowController.generateMore(
      workflowState.currentPrompt,
      workflowState.negativePrompt,
      batchOffsetRef.current,
    );
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
    singleAssetWorkflowController.reset();
    clearSingleAssetSnapshot();
    setRestoredSnapshotMode(false);
    setRestoredApprovalRefreshPending(false);
    setUploadedImageB64("");
    setUploadedImageName("");
    setUploadedImagePreview(null);
    resetProjectCreationState();
    batchOffsetRef.current = 0;
  }

  async function handleApprovalAction(
    actionId: string,
    action: (id: string) => Promise<ApprovalRequest>,
  ) {
    await runApprovalAction({
      actionId,
      action,
      onBusyChange(nextActionId) {
        dispatchWorkflow({ type: "approval_busy_set", actionId: nextActionId });
      },
      onSuccess(updated) {
        dispatchWorkflow({
          type: "approval_requests_updated",
          requests: workflowState.approvalRequests.map(req => req.action_id === actionId ? updated : req),
        });
      },
      onError(message) {
        dispatchWorkflow({
          type: "workflow_failed",
          message,
          traceback: null,
        });
      },
    });
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
          errorMessage={workflowState.errorMessage}
          errorTraceback={workflowState.errorTraceback}
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
          projectCreateBusy={projectCreateBusy}
          projectCreateMessage={projectCreateMessage}
          projectCreateError={projectCreateError}
          onCreateProject={() => {
            void createProjectAtRoot(projectRoot).catch(() => {});
          }}
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
            singleAssetWorkflowController.proceedApproval();
          }}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      )}

      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
