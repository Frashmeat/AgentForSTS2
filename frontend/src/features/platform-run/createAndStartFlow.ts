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
    const workspace = await createMyServerWorkspace({
      project_name: request.serverWorkspaceProjectName.trim(),
    });
    for (const item of items) {
      item.input_payload.server_project_ref = workspace.server_project_ref;
    }
  }

  for (const upload of request.serverUploads ?? []) {
    const uploaded = await uploadMyServerAsset({
      file_name: upload.fileName,
      content_base64: upload.contentBase64,
      mime_type: upload.mimeType,
    });
    if (items[upload.itemIndex]) {
      items[upload.itemIndex].input_payload.uploaded_asset_ref = uploaded.uploaded_asset_ref;
    }
  }

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

  const startConfirmed = (await request.confirmStart?.(job)) ?? true;
  if (!startConfirmed) {
    return {
      job,
      started: null,
      deferredNotice: null,
      startConfirmed: false,
    };
  }

  const started = await startMyJob(job.id, {
    triggered_by: "user",
  });
  const deferredNotice =
    started?.status === "deferred" ? readDeferredExecutionNotice(await listMyJobEvents(job.id)) : null;

  return {
    job,
    started,
    deferredNotice,
    startConfirmed: true,
  };
}
