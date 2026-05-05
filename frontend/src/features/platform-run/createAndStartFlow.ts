import {
  createMyServerWorkspace,
  createMyJob,
  listMyJobEvents,
  startMyJob,
  uploadMyServerAsset,
} from "../../shared/api/me.ts";
import type { PlatformJobCreateItem, PlatformJobSummary } from "../../shared/api/platform.ts";
import { readDeferredExecutionNotice, type DeferredExecutionNotice } from "../../shared/deferredExecution.ts";

export type PlatformRunProgressStage =
  | "preparing_workspace"
  | "uploading_asset"
  | "creating_job"
  | "job_created"
  | "starting_job"
  | "queued"
  | "completed";

export interface PlatformRunProgressUpdate {
  stage: PlatformRunProgressStage;
  message: string;
  jobId?: number;
}

export interface CreateAndStartPlatformFlowRequest {
  jobType: string;
  workflowVersion: string;
  inputSummary: string;
  createdFrom: string;
  items: PlatformJobCreateItem[];
  serverUploads?: Array<{
    itemIndex: number;
    fileName: string;
    contentBase64: string;
    mimeType?: string;
  }>;
  serverWorkspaceProjectName?: string;
  selectedExecutionProfileId?: number;
  selectedRunnerType?: string;
  selectedModel?: string;
  confirmStart?: (job: PlatformJobSummary) => boolean | Promise<boolean>;
  onProgress?: (update: PlatformRunProgressUpdate) => void;
}

export interface CreateAndStartPlatformFlowResult {
  job: PlatformJobSummary;
  started: {
    id?: number;
    status?: string;
    ok?: boolean;
  } | null;
  deferredNotice: DeferredExecutionNotice | null;
  startConfirmed: boolean;
}

const SERVER_WORKSPACE_JOB_TYPES = new Set(["single_generate", "batch_generate"]);

function normalizeProjectName(value: string) {
  const normalized = value.trim().replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "");
  return normalized || "GeneratedMod";
}

function inferServerWorkspaceProjectName(request: CreateAndStartPlatformFlowRequest) {
  const explicitName = request.serverWorkspaceProjectName?.trim();
  if (explicitName) {
    return explicitName;
  }
  const firstItem = request.items[0];
  const itemName = String(firstItem?.input_payload?.item_name ?? "").trim();
  if (itemName) {
    return normalizeProjectName(itemName);
  }
  return normalizeProjectName(request.inputSummary);
}

export async function createAndStartPlatformFlow(
  request: CreateAndStartPlatformFlowRequest,
): Promise<CreateAndStartPlatformFlowResult> {
  const items = request.items.map((item) => ({
    ...item,
    input_payload: { ...(item.input_payload ?? {}) },
  }));

  const serverWorkspaceProjectName = SERVER_WORKSPACE_JOB_TYPES.has(request.jobType)
    ? inferServerWorkspaceProjectName(request)
    : "";
  if (serverWorkspaceProjectName) {
    request.onProgress?.({ stage: "preparing_workspace", message: "正在创建 Web 托管工作站项目区" });
    const workspace = await createMyServerWorkspace({
      project_name: serverWorkspaceProjectName,
    });
    for (const item of items) {
      item.input_payload.server_project_ref = workspace.server_project_ref;
    }
  }

  for (const upload of request.serverUploads ?? []) {
    request.onProgress?.({ stage: "uploading_asset", message: `正在上传托管资产：${upload.fileName}` });
    const uploaded = await uploadMyServerAsset({
      file_name: upload.fileName,
      content_base64: upload.contentBase64,
      mime_type: upload.mimeType,
    });
    if (items[upload.itemIndex]) {
      items[upload.itemIndex].input_payload.uploaded_asset_ref = uploaded.uploaded_asset_ref;
    }
  }

  request.onProgress?.({ stage: "creating_job", message: "正在创建平台任务" });
  const job = await createMyJob({
    job_type: request.jobType,
    workflow_version: request.workflowVersion,
    input_summary: request.inputSummary,
    created_from: request.createdFrom,
    items,
    selected_execution_profile_id: request.selectedExecutionProfileId,
    selected_runner_type: request.selectedRunnerType,
    selected_model: request.selectedModel,
  });

  request.onProgress?.({ stage: "job_created", message: `平台任务 #${job.id} 已创建，等待开始确认`, jobId: job.id });
  const startConfirmed = (await request.confirmStart?.(job)) ?? true;
  if (!startConfirmed) {
    return {
      job,
      started: null,
      deferredNotice: null,
      startConfirmed: false,
    };
  }

  request.onProgress?.({ stage: "starting_job", message: `正在启动平台任务 #${job.id}`, jobId: job.id });
  const started = await startMyJob(job.id, {
    triggered_by: "user",
  });
  request.onProgress?.({ stage: "queued", message: `平台任务 #${job.id} 已提交到 Web 托管工作站队列`, jobId: job.id });
  const deferredNotice =
    started?.status === "deferred" ? readDeferredExecutionNotice(await listMyJobEvents(job.id)) : null;

  return {
    job,
    started,
    deferredNotice,
    startConfirmed: true,
  };
}
