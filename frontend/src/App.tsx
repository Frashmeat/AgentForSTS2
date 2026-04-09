import { useState, useRef, useEffect, useReducer } from "react";
import { Bug, House, LayoutDashboard, Sparkles, Wrench } from "lucide-react";
import { Link, Navigate, Route, Routes, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import ExecutionModeDialog from "./components/ExecutionModeDialog.tsx";
import { KnowledgeGuideDialog } from "./components/KnowledgeGuideDialog.tsx";
import { PlatformAuthUnavailableNotice } from "./components/PlatformAuthUnavailableNotice.tsx";
import { PlatformPageShell } from "./components/platform/PlatformPageShell.tsx";
import { WorkspaceShell } from "./components/workspace/WorkspaceShell.tsx";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "./shared/api/index.ts";
import { type SingleAssetSocket } from "./lib/single_asset_ws";
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
import { createAndStartPlatformFlow } from "./features/platform-run/createAndStartFlow.ts";
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
import {
  getRefreshKnowledgeTask,
  loadKnowledgeStatus,
  loadLocalAiCapabilityStatus,
  resolveMigrationFlags,
  startRefreshKnowledgeTask,
  type KnowledgeStatus,
  type LocalAiCapabilityStatus,
  type WorkflowMigrationFlags,
} from "./shared/api/index.ts";
import type { PlatformJobCreateItem } from "./shared/api/platform.ts";
import { runApprovalAction } from "./shared/approvalAction.ts";
import { useSession } from "./shared/session/hooks.ts";
import { useDefaultProjectRoot } from "./shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "./shared/useProjectCreation.ts";
import { SettingsPage } from "./pages/SettingsPage.tsx";

type AppTab = WorkspaceTab;

const PLATFORM_WORKFLOW_VERSION = "2026.04.04";
const workspaceNavItems = [
  {
    id: "single" as const,
    label: "单资产",
    shortLabel: "单资产",
    description: "描述、生成、审批和构建单个资产。",
    icon: Sparkles,
  },
  {
    id: "batch" as const,
    label: "Mod 规划",
    shortLabel: "规划",
    description: "批量规划多个资产，并跟踪每个条目的执行状态。",
    icon: LayoutDashboard,
  },
  {
    id: "edit" as const,
    label: "修改 Mod",
    shortLabel: "修改",
    description: "分析现有项目结构，并让 Code Agent 执行改动。",
    icon: Wrench,
  },
  {
    id: "log" as const,
    label: "崩溃分析",
    shortLabel: "日志",
    description: "读取最近日志并生成故障定位建议。",
    icon: Bug,
  },
];

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

function buildSettingsPath(returnTo: string): string {
  const nextSearch = new URLSearchParams({ returnTo });
  return `/settings?${nextSearch.toString()}`;
}

function buildPlatformAuthUnavailableElement(title: string, description: string) {
  return (
    <PlatformPageShell
      kicker="Platform Access"
      title={title}
      description="当前环境还没有接入独立 Web 平台服务，因此认证与用户中心能力不可用。"
      actions={
        <Link to="/" className="platform-page-action-link">
          <House size={16} />
          <span>返回首页</span>
        </Link>
      }
      width="narrow"
    >
      <PlatformAuthUnavailableNotice title={title} description={description} />
    </PlatformPageShell>
  );
}

interface PendingExecutionRequest extends PlatformExecutionRequest {
  localAvailable: boolean;
  localUnavailableReasons: string[];
}

export default function App() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { isAuthAvailable, isAuthenticated } = useSession();
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
  const [knowledgeStatus, setKnowledgeStatus] = useState<KnowledgeStatus | null>(null);
  const [knowledgeGuideOpen, setKnowledgeGuideOpen] = useState(false);
  const [knowledgeRefreshTaskId, setKnowledgeRefreshTaskId] = useState("");
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
    void loadKnowledgeStatus()
      .then((status) => {
        setKnowledgeStatus(status);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!knowledgeRefreshTaskId) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollKnowledgeRefresh() {
      try {
        const snapshot = await getRefreshKnowledgeTask(knowledgeRefreshTaskId);
        if (cancelled) {
          return;
        }
        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollKnowledgeRefresh, 800);
          return;
        }
        const status = await loadKnowledgeStatus();
        if (!cancelled) {
          setKnowledgeStatus(status);
        }
        setKnowledgeRefreshTaskId("");
      } catch {
        if (cancelled) {
          return;
        }
        setKnowledgeRefreshTaskId("");
      }
    }

    void pollKnowledgeRefresh();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [knowledgeRefreshTaskId]);

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
    let capability: LocalAiCapabilityStatus;
    try {
      capability = await loadLocalAiCapabilityStatus();
    } catch {
      capability = {
        text_ai_available: false,
        code_agent_available: false,
        image_ai_available: false,
        text_ai_missing_reasons: ["无法读取本机配置状态，请检查工作站后端是否正常运行。"],
        code_agent_missing_reasons: ["无法读取本机代码代理状态，请检查工作站后端是否正常运行。"],
        image_ai_missing_reasons: [],
      };
    }

    const localUnavailableReasons = [
      ...(capability.text_ai_available ? [] : capability.text_ai_missing_reasons ?? []),
      ...(request.requiresCodeAgent && !capability.code_agent_available ? capability.code_agent_missing_reasons ?? [] : []),
      ...(
        request.requiresImageAi && !capability.image_ai_available
          ? capability.image_ai_missing_reasons ?? []
          : []
      ),
    ];

    setPendingExecution({
      ...request,
      localAvailable:
        capability.text_ai_available &&
        (!request.requiresCodeAgent || capability.code_agent_available) &&
        (!request.requiresImageAi || capability.image_ai_available),
      localUnavailableReasons,
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

    const request = pendingExecution;
    if (!isAuthenticated) {
      handleGoLoginForServerExecution();
      return;
    }

    setPendingExecution(null);
    try {
      const result = await createAndStartPlatformFlow({
        jobType: request.jobType,
        workflowVersion: PLATFORM_WORKFLOW_VERSION,
        inputSummary: request.inputSummary,
        createdFrom: request.createdFrom,
        items: request.items,
        confirmStart(job) {
          return window.confirm(
            `已创建平台任务 #${job.id}。确认开始后会进入服务器队列，并按平台规则计费。是否继续开始？`,
          );
        },
      });
      navigate(`/me/jobs/${result.job.id}`);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "创建平台任务失败");
    }
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

  async function handleRefreshKnowledge() {
    setKnowledgeStatus((prev) => (
      prev
        ? { ...prev, status: "refreshing" }
        : {
            status: "refreshing",
            generated_at: null,
            checked_at: null,
            warnings: [],
            game: {},
            baselib: {},
          }
    ));
    try {
      const task = await startRefreshKnowledgeTask();
      setKnowledgeRefreshTaskId(task.task_id);
    } catch {
      try {
        const status = await loadKnowledgeStatus();
        setKnowledgeStatus(status);
      } catch {
        setKnowledgeStatus((prev) => prev ? { ...prev, status: "error" } : prev);
      }
    }
  }

  function renderWorkspaceContent() {
    const openSettingsPage = () => {
      navigate(buildSettingsPath(buildWorkspacePath(activeTab)));
    };
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
        {activeTab === "batch" && (
          <div className="px-4 py-4 sm:px-6 sm:py-6">
            <BatchGenerationFeatureView onRequestExecution={handleExecutionRequest} />
          </div>
        )}

        {activeTab === "edit" && (
          <div className="px-4 py-4 sm:px-6 sm:py-6">
            <ModEditorFeatureView onRequestExecution={handleExecutionRequest} />
          </div>
        )}

        {activeTab === "log" && (
          <div className="px-4 py-4 sm:px-6 sm:py-6">
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
            agentLogEntries={workflowState.agentLogEntries}
            currentAgentModel={workflowState.currentAgentModel}
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
            knowledgeStatus={knowledgeStatus}
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
                requiresCodeAgent: true,
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
            onRefreshKnowledge={() => {
              void handleRefreshKnowledge();
            }}
            onOpenKnowledgeGuide={() => setKnowledgeGuideOpen(true)}
            onOpenSettings={openSettingsPage}
          />
        )}
      </>
    );
  }

  return (
    <div className="min-h-screen bg-[var(--workspace-bg)] text-slate-800">
      <Routes>
        <Route
          path="/"
          element={
            <WorkspaceShell
              activeTab={activeTab}
              navItems={workspaceNavItems}
              onTabChange={updateActiveTab}
              onOpenSettings={() => navigate(buildSettingsPath(buildWorkspacePath(activeTab)))}
            >
              {renderWorkspaceContent()}
            </WorkspaceShell>
          }
        />
        <Route path="/settings" element={<SettingsPage />} />
        <Route
          path="/auth/login"
          element={isAuthAvailable ? <LoginPage /> : buildPlatformAuthUnavailableElement("当前环境不支持登录", "这是本机工作站模式，未接入独立 Web 平台服务，因此登录与注册入口不可用。")}
        />
        <Route
          path="/auth/register"
          element={isAuthAvailable ? <RegisterPage /> : buildPlatformAuthUnavailableElement("当前环境不支持注册", "这是本机工作站模式，未接入独立 Web 平台服务，因此无法创建平台账号。")}
        />
        <Route
          path="/auth/verify-email"
          element={isAuthAvailable ? <VerifyEmailPage /> : buildPlatformAuthUnavailableElement("当前环境不支持邮箱验证", "请先接入独立 Web 平台服务，再使用平台账号验证链路。")}
        />
        <Route
          path="/auth/forgot-password"
          element={isAuthAvailable ? <ForgotPasswordPage /> : buildPlatformAuthUnavailableElement("当前环境不支持密码找回", "请先接入独立 Web 平台服务，再使用平台账号密码找回功能。")}
        />
        <Route
          path="/auth/reset-password"
          element={isAuthAvailable ? <ResetPasswordPage /> : buildPlatformAuthUnavailableElement("当前环境不支持密码重置", "请先接入独立 Web 平台服务，再使用平台账号密码重置功能。")}
        />
        <Route
          path="/me"
          element={isAuthAvailable ? <UserCenterPage /> : buildPlatformAuthUnavailableElement("当前环境未启用用户中心", "这是本机工作站模式。只有接入独立 Web 平台服务后，用户中心、任务记录和次数池才可用。")}
        />
        <Route
          path="/me/jobs/:jobId"
          element={isAuthAvailable ? <UserCenterJobDetailPage /> : buildPlatformAuthUnavailableElement("当前环境未启用用户中心", "这是本机工作站模式。只有接入独立 Web 平台服务后，平台任务详情才可用。")}
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <KnowledgeGuideDialog open={knowledgeGuideOpen} status={knowledgeStatus} onClose={() => setKnowledgeGuideOpen(false)} />
      <ExecutionModeDialog
        open={pendingExecution !== null}
        title={pendingExecution?.title ?? "选择执行方式"}
        localAvailable={pendingExecution?.localAvailable ?? false}
        localUnavailableReasons={pendingExecution?.localUnavailableReasons ?? []}
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
