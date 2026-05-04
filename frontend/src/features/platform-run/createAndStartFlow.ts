import {
  createMyServerWorkspace,
  createMyJob,
  listMyJobEvents,
  startMyJob,
  uploadMyServerAsset,
} from "../../shared/api/me.ts";
import type { PlatformJobCreateItem, PlatformJobSummary } from "../../shared/api/platform.ts";
import { readDeferredExecutionNotice, type DeferredExecutionNotice } from "../../shared/deferredExecution.ts";

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
  selectedAgentBackend?: string;
  selectedModel?: string;
  confirmStart?: (job: PlatformJobSummary) => boolean | Promise<boolean>;
  onProgress?: (message: string) => void;
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

export async function createAndStartPlatformFlow(
  request: CreateAndStartPlatformFlowRequest,
): Promise<CreateAndStartPlatformFlowResult> {
  const items = request.items.map((item) => ({
    ...item,
    input_payload: { ...(item.input_payload ?? {}) },
  }));

  if (request.serverWorkspaceProjectName?.trim()) {
    request.onProgress?.("正在创建服务器工作区...");
    const workspace = await createMyServerWorkspace({
      project_name: request.serverWorkspaceProjectName.trim(),
    });
    for (const item of items) {
      item.input_payload.server_project_ref = workspace.server_project_ref;
    }
  }

  for (const upload of request.serverUploads ?? []) {
    request.onProgress?.(`正在上传服务器资产：${upload.fileName}`);
    const uploaded = await uploadMyServerAsset({
      file_name: upload.fileName,
      content_base64: upload.contentBase64,
      mime_type: upload.mimeType,
    });
    if (items[upload.itemIndex]) {
      items[upload.itemIndex].input_payload.uploaded_asset_ref = uploaded.uploaded_asset_ref;
    }
  }

  request.onProgress?.("正在创建平台任务...");
  const job = await createMyJob({
    job_type: request.jobType,
    workflow_version: request.workflowVersion,
    input_summary: request.inputSummary,
    created_from: request.createdFrom,
    items,
    selected_execution_profile_id: request.selectedExecutionProfileId,
    selected_agent_backend: request.selectedAgentBackend,
    selected_model: request.selectedModel,
  });

  request.onProgress?.(`平台任务 #${job.id} 已创建，等待开始确认...`);
  const startConfirmed = (await request.confirmStart?.(job)) ?? true;
  if (!startConfirmed) {
    return {
      job,
      started: null,
      deferredNotice: null,
      startConfirmed: false,
    };
  }

  request.onProgress?.(`正在启动平台任务 #${job.id}...`);
  const started = await startMyJob(job.id, {
    triggered_by: "user",
  });
  request.onProgress?.(`平台任务 #${job.id} 已提交到服务器队列。`);
  const deferredNotice =
    started?.status === "deferred" ? readDeferredExecutionNotice(await listMyJobEvents(job.id)) : null;

  return {
    job,
    started,
    deferredNotice,
    startConfirmed: true,
  };
}
