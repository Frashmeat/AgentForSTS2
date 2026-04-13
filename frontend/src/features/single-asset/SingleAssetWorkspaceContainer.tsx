import { useEffect, useReducer, useRef, useState } from "react";

import { type SingleAssetSocket } from "../../lib/single_asset_ws.ts";
import {
  approveApproval,
  executeApproval,
  rejectApproval,
  type ApprovalRequest,
} from "../../shared/api/index.ts";
import type { PlatformJobCreateItem } from "../../shared/api/platform.ts";
import { runApprovalAction } from "../../shared/approvalAction.ts";
import { useDefaultProjectRoot } from "../../shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "../../shared/useProjectCreation.ts";
import { useWorkspaceContext } from "../workspace/WorkspaceContext.tsx";
import { createSingleAssetWorkflowController } from "./controller.ts";
import { type AssetType, getStageIndex } from "./model.ts";
import {
  clearSingleAssetSnapshot,
  hasSingleAssetRecoveryContext,
  loadSingleAssetSnapshot,
  refreshRecoveredSingleAssetApprovals,
  saveSingleAssetSnapshot,
} from "./recovery.ts";
import { createInitialSingleAssetWorkflowState, singleAssetWorkflowReducer } from "./state.ts";
import { SingleAssetFeatureView } from "./view.tsx";

export function SingleAssetWorkspaceContainer() {
  const {
    knowledgeStatus,
    onOpenKnowledgeGuide,
    onOpenSettings,
    onRefreshKnowledge,
    onRequestExecution,
  } = useWorkspaceContext();
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

  useEffect(() => {
    autoModeRef.current = autoMode;
  }, [autoMode]);

  useDefaultProjectRoot({
    setProjectRoot,
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
    assetName,
    assetType,
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
    reader.onload = (event) => {
      const dataUrl = event.target?.result as string;
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
          requests: workflowState.approvalRequests.map((request) => (
            request.action_id === actionId ? updated : request
          )),
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
        void onRequestExecution({
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
        void handleApprovalAction(actionId, rejectApproval);
      }}
      onExecute={(actionId) => {
        void handleApprovalAction(actionId, executeApproval);
      }}
      onProceedApproval={() => {
        singleAssetWorkflowController.proceedApproval();
      }}
      onRefreshKnowledge={onRefreshKnowledge}
      onOpenKnowledgeGuide={onOpenKnowledgeGuide}
      onOpenSettings={onOpenSettings}
    />
  );
}
