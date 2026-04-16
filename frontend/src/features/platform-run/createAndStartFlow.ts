import {
  createMyJob,
  listMyJobEvents,
  startMyJob,
} from "../../shared/api/me.ts";
import type {
  PlatformJobCreateItem,
  PlatformJobSummary,
} from "../../shared/api/platform.ts";
import {
  readDeferredExecutionNotice,
  type DeferredExecutionNotice,
} from "../../shared/deferredExecution.ts";

export interface CreateAndStartPlatformFlowRequest {
  jobType: string;
  workflowVersion: string;
  inputSummary: string;
  createdFrom: string;
  items: PlatformJobCreateItem[];
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
  const job = await createMyJob({
    job_type: request.jobType,
    workflow_version: request.workflowVersion,
    input_summary: request.inputSummary,
    created_from: request.createdFrom,
    items: request.items,
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
  const deferredNotice = started?.status === "deferred"
    ? readDeferredExecutionNotice(await listMyJobEvents(job.id))
    : null;

  return {
    job,
    started,
    deferredNotice,
    startConfirmed: true,
  };
}
