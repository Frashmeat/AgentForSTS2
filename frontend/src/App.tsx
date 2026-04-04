import { useState, useRef, useEffect, useReducer } from "react";
import { Settings, Swords } from "lucide-react";
import { Navigate, Route, Routes, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import ExecutionModeDialog from "./components/ExecutionModeDialog.tsx";
import { SettingsPanel } from "./components/SettingsPanel";
import { UserEntry } from "./components/UserEntry.tsx";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "./shared/api/index.ts";
import { type SingleAssetSocket } from "./lib/single_asset_ws";
import { cn } from "./lib/utils";
import { BatchGenerationFeatureView } from "./features/batch-generation/view";
import { ForgotPasswordPage } from "./features/auth/ForgotPasswordPage.tsx";
import { LoginPage } from "./features/auth/LoginPage.tsx";
import { RegisterPage } from "./features/auth/RegisterPage.tsx";
import { ResetPasswordPage } from "./features/auth/ResetPasswordPage.tsx";
import { VerifyEmailPage } from "./features/auth/VerifyEmailPage.tsx";
import { UserCenterJobDetailPage } from "./features/user-center/job-detail-page.tsx";
import { UserCenterPage } from "./features/user-center/page.tsx";
import { LogAnalysisFeatureView } from "./features/log-analysis/view";
import { ModEditorFeatureView } from "./features/mod-editor/view";
import type { PlatformExecutionRequest, WorkspaceTab } from "./features/platform-run/types.ts";
import { type AssetType, getStageIndex } from "./features/single-asset/model";
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
import { loadLocalAiCapabilityStatus, resolveMigrationFlags, type WorkflowMigrationFlags } from "./shared/api/index.ts";
import type { PlatformJobCreateItem } from "./shared/api/platform.ts";
import { runApprovalAction } from "./shared/approvalAction.ts";
import { useSession } from "./shared/session/hooks.ts";
import { useDefaultProjectRoot } from "./shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "./shared/useProjectCreation.ts";

type AppTab = WorkspaceTab;

function resolveAppTab(value: string | null): AppTab {
  switch (value) {
    case "batch":
    case "edit":
    case "log":
      return value;
    default:
      return "single";
  }
}

function buildWorkspacePath(tab: AppTab): string {
  return tab === "single" ? "/" : `/?tab=${tab}`;
}

interface PendingExecutionRequest extends PlatformExecutionRequest {
  localAvailable: boolean;
}

export default function App() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { isAuthenticated } = useSession();
  const activeTab = resolveAppTab(searchParams.get("tab"));
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
  const [pendingExecution, setPendingExecution] = useState<PendingExecutionRequest | null>(null);
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

  function updateActiveTab(nextTab: AppTab) {
    if (location.pathname !== "/") {
      return;
    }
    const nextSearchParams = new URLSearchParams(searchParams);
    if (nextTab === "single") {
      nextSearchParams.delete("tab");
    } else {
      nextSearchParams.set("tab", nextTab);
    }
    setSearchParams(nextSearchParams, { replace: true });
  }

  async function handleExecutionRequest(request: PlatformExecutionRequest) {
    let capability;
    try {
      capability = await loadLocalAiCapabilityStatus();
    } catch {
      capability = {
        text_ai_available: false,
        image_ai_available: false,
      };
    }

    setPendingExecution({
      ...request,
      localAvailable: capability.text_ai_available && (!request.requiresImageAi || capability.image_ai_available),
    });
  }

  function handleChooseLocalExecution() {
    if (pendingExecution === null) {
      return;
    }
    const request = pendingExecution;
    setPendingExecution(null);
    request.runLocal();
  }

  function handleGoLoginForServerExecution() {
    if (pendingExecution === null) {
      return;
    }
    const request = pendingExecution;
    setPendingExecution(null);
    navigate("/auth/login", {
      replace: true,
      state: {
        redirectTo: buildWorkspacePath(request.tab),
      },
    });
  }

  async function handleChooseServerExecution() {
    if (pendingExecution === null) {
      return;
    }

    if (!isAuthenticated) {
      handleGoLoginForServerExecution();
      return;
    }

    setPendingExecution(null);
    navigate("/me");
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

  function renderWorkspaceShell() {
    const singleAssetRequiresImageAi = imageMode === "ai";
    const singleAssetInputSummary = `${assetType}:${assetName.trim() || "未命名资产"}`;
    const singleAssetItem: PlatformJobCreateItem = {
      item_type: assetType,
      input_summary: description.trim() || singleAssetInputSummary,
      input_payload: {
        asset_type: assetType,
        asset_name: assetName.trim(),
        description: description.trim(),
        project_root: projectRoot.trim(),
        image_mode: imageMode,
        auto_mode: autoMode,
        has_uploaded_image: Boolean(uploadedImageB64),
      },
    };

    return (
      <>
        <div className="px-6 pt-4 flex gap-1 border-b border-slate-200 bg-white">
          <button
            onClick={() => updateActiveTab("single")}
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
            onClick={() => updateActiveTab("batch")}
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
            onClick={() => updateActiveTab("edit")}
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
            onClick={() => updateActiveTab("log")}
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
            <BatchGenerationFeatureView onRequestExecution={handleExecutionRequest} />
          </div>
        )}

        {activeTab === "edit" && (
          <div className="px-6 py-6">
            <ModEditorFeatureView onRequestExecution={handleExecutionRequest} />
          </div>
        )}

        {activeTab === "log" && (
          <div className="px-6 py-6">
            <LogAnalysisFeatureView onRequestExecution={handleExecutionRequest} />
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
            onStartWorkflow={() => {
              void handleExecutionRequest({
                title: "开始生成资产",
                tab: "single",
                jobType: "single_generate",
                createdFrom: "single_asset",
                inputSummary: singleAssetInputSummary,
                requiresImageAi: singleAssetRequiresImageAi,
                items: [singleAssetItem],
                runLocal() {
                  void startWorkflow();
                },
              });
            }}
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
      </>
    );
  }

  const isWorkspaceRoute = location.pathname === "/";

  return (
    <div className="min-h-screen bg-slate-50 text-slate-800">
      <header className="sticky top-0 z-10 border-b border-slate-200 px-6 py-3 flex items-center justify-between bg-white/80 backdrop-blur-sm shadow-sm">
        <div className="flex items-center gap-2">
          <Swords className="text-amber-600" size={22} />
          <span className="font-bold tracking-wide text-amber-600 text-lg">AgentTheSpire</span>
        </div>
        <div className="flex items-center gap-3">
          <UserEntry />
          {isWorkspaceRoute && (
            <button onClick={() => setSettingsOpen(true)} className="flex items-center gap-1.5 py-1.5 px-3 rounded-lg bg-slate-100 hover:bg-amber-50 hover:text-amber-700 text-slate-500 hover:border-amber-300 border border-transparent transition-colors text-sm font-medium">
              <Settings size={14} />
              设置
            </button>
          )}
        </div>
      </header>

      <Routes>
        <Route path="/" element={renderWorkspaceShell()} />
        <Route path="/auth/login" element={<div className="px-6 py-10"><LoginPage /></div>} />
        <Route path="/auth/register" element={<div className="px-6 py-10"><RegisterPage /></div>} />
        <Route path="/auth/verify-email" element={<div className="px-6 py-10"><VerifyEmailPage /></div>} />
        <Route path="/auth/forgot-password" element={<div className="px-6 py-10"><ForgotPasswordPage /></div>} />
        <Route path="/auth/reset-password" element={<div className="px-6 py-10"><ResetPasswordPage /></div>} />
        <Route path="/me" element={<UserCenterPage />} />
        <Route path="/me/jobs/:jobId" element={<UserCenterJobDetailPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>

      {settingsOpen && isWorkspaceRoute && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      <ExecutionModeDialog
        open={pendingExecution !== null}
        title={pendingExecution?.title ?? "选择执行方式"}
        localAvailable={pendingExecution?.localAvailable ?? false}
        isAuthenticated={isAuthenticated}
        onClose={() => setPendingExecution(null)}
        onChooseLocal={handleChooseLocalExecution}
        onChooseServer={() => {
          void handleChooseServerExecution();
        }}
        onGoLogin={handleGoLoginForServerExecution}
      />
    </div>
  );
}
