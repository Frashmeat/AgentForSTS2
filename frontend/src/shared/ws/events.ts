import type { WorkflowEvent } from "../types/workflow.ts";

export function normalizeEvent(payload: Record<string, unknown>): WorkflowEvent {
  const event = String(payload.event ?? payload.stage ?? "unknown");
  return {
    ...payload,
    event,
    stage: String(payload.stage ?? event),
  } as WorkflowEvent;
}
