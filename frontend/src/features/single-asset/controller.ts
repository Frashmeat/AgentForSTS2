import {
  createSingleAssetSocket,
  type SingleAssetSocket,
} from "../../lib/single_asset_ws.ts";
import type { WorkflowMigrationFlags } from "../../shared/api/index.ts";
import type { SingleAssetWorkflowAction } from "./state.ts";

export interface SingleAssetWorkflowSocketLike extends SingleAssetSocket {}

export interface StartSingleAssetWorkflowOptions {
  assetType: string;
  assetName: string;
  description: string;
  projectRoot: string;
  imageMode: "ai" | "upload";
  uploadedImageB64: string;
  uploadedImageName: string;
  autoMode: boolean;
  migrationFlags: WorkflowMigrationFlags;
}

export interface SingleAssetWorkflowRuntime {
  closeSocket(): void;
  setSocket(socket: SingleAssetWorkflowSocketLike | null): void;
  getSocket(): SingleAssetWorkflowSocketLike | null;
  clearProjectCreationFeedback(): void;
  setRestoredSnapshotMode(value: boolean): void;
  setRestoredApprovalRefreshPending(value: boolean): void;
  dispatchWorkflow(action: SingleAssetWorkflowAction): void;
  reportWorkflowError(message: string, traceback: string | null): void;
}

interface SingleAssetWorkflowDeps {
  createSocket(flags: WorkflowMigrationFlags): SingleAssetWorkflowSocketLike;
}

export function createSingleAssetWorkflowController(
  runtime: SingleAssetWorkflowRuntime,
  deps: Partial<SingleAssetWorkflowDeps> = {},
) {
  const createSocket = deps.createSocket ?? createSingleAssetSocket;
  let batchOffset = 0;

  function reset() {
    batchOffset = 0;
    runtime.closeSocket();
    runtime.setSocket(null);
    runtime.dispatchWorkflow({ type: "workflow_reset" });
  }

  function confirmPrompt(prompt: string, negativePrompt: string) {
    const socket = runtime.getSocket();
    if (!socket) {
      return;
    }
    runtime.dispatchWorkflow({ type: "prompt_confirmed" });
    socket.send({ action: "confirm", prompt, negative_prompt: negativePrompt });
  }

  function selectImage(index: number) {
    const socket = runtime.getSocket();
    if (!socket) {
      return;
    }
    runtime.dispatchWorkflow({ type: "image_selected" });
    socket.send({ action: "select", index });
  }

  function generateMore(prompt: string, negativePrompt: string, nextBatchOffset: number) {
    const socket = runtime.getSocket();
    if (!socket) {
      return;
    }
    batchOffset = nextBatchOffset;
    runtime.dispatchWorkflow({ type: "generate_more_requested", batchOffset });
    socket.send({
      action: "generate_more",
      prompt,
      negative_prompt: negativePrompt || undefined,
    });
  }

  function proceedApproval() {
    const socket = runtime.getSocket();
    if (!socket) {
      return;
    }
    socket.send({ action: "approve_all" });
  }

  async function start(options: StartSingleAssetWorkflowOptions) {
    const {
      assetType,
      assetName,
      description,
      projectRoot,
      imageMode,
      uploadedImageB64,
      uploadedImageName,
      autoMode,
      migrationFlags,
    } = options;

    const normalizedProjectRoot = projectRoot.trim();
    if (!assetName.trim() || !description.trim() || !normalizedProjectRoot) {
      return;
    }

    batchOffset = 0;
    runtime.clearProjectCreationFeedback();
    runtime.setRestoredSnapshotMode(false);
    runtime.setRestoredApprovalRefreshPending(false);
    runtime.closeSocket();
    runtime.setSocket(null);
    runtime.dispatchWorkflow({ type: "workflow_started", imageMode });

    const socket = createSocket(migrationFlags);
    runtime.setSocket(socket);
    socket.on("stage_update", (message) => {
      runtime.dispatchWorkflow({
        type: message.scope === "agent" ? "agent_stage_pushed" : "flow_stage_pushed",
        message: message.message,
      });
    });
    socket.on("progress", (message) => {
      runtime.dispatchWorkflow({
        type: "gen_log_appended",
        message: `${message.message}`,
      });
    });
    socket.on("agent_stream", (message) => {
      runtime.dispatchWorkflow({
        type: "agent_log_appended",
        message: message.chunk,
      });
    });
    socket.on("error", (message) => {
      runtime.reportWorkflowError(message.message, message.traceback || null);
    });
    socket.on("approval_pending", (message) => {
      runtime.dispatchWorkflow({
        type: "approval_pending_received",
        summary: message.summary || "",
        requests: message.requests || [],
      });
    });
    socket.on("prompt_preview", (message) => {
      if (autoMode) {
        socket.send({
          action: "confirm",
          prompt: message.prompt,
          negative_prompt: message.negative_prompt || "",
        });
        runtime.dispatchWorkflow({
          type: "gen_log_appended",
          message: "自动模式：跳过 prompt 确认",
        });
        return;
      }
      runtime.dispatchWorkflow({
        type: "prompt_preview_received",
        prompt: message.prompt,
        negativePrompt: message.negative_prompt || "",
        fallbackWarning: message.fallback_warning || null,
      });
    });
    socket.on("image_ready", (message) => {
      runtime.dispatchWorkflow({
        type: "image_ready_received",
        index: message.index,
        image: message.image,
        prompt: message.prompt,
        batchOffset,
      });
      if (autoMode) {
        runtime.dispatchWorkflow({
          type: "gen_log_appended",
          message: "自动模式：自动选第 1 张图",
        });
        socket.send({ action: "select", index: 0 });
        runtime.dispatchWorkflow({ type: "stage_changed", stage: "agent_running" });
      }
    });
    socket.on("done", (message) => {
      runtime.dispatchWorkflow({
        type: "agent_log_appended",
        message: message.success ? "✓ 构建成功！" : "✗ 构建失败",
      });
      runtime.dispatchWorkflow({ type: "stage_changed", stage: "done" });
    });

    try {
      await socket.waitOpen();
    } catch (error) {
      runtime.setSocket(null);
      runtime.reportWorkflowError(error instanceof Error ? error.message : String(error), null);
      return;
    }

    if (imageMode === "upload" && uploadedImageB64) {
      runtime.dispatchWorkflow({ type: "stage_changed", stage: "agent_running" });
      socket.send({
        action: "start",
        asset_type: assetType,
        asset_name: assetName,
        description,
        project_root: normalizedProjectRoot,
        provided_image_b64: uploadedImageB64,
        provided_image_name: uploadedImageName,
      });
      return;
    }

    socket.send({
      action: "start",
      asset_type: assetType,
      asset_name: assetName,
      description,
      project_root: normalizedProjectRoot,
    });
  }

  return {
    start,
    confirmPrompt,
    selectImage,
    generateMore,
    proceedApproval,
    reset,
  };
}
