import type { WorkflowEvent, ModPlan, PlanItem } from "../shared/types/workflow.ts";
import { WorkflowSocketFacade } from "../shared/ws/facade.ts";

export type BatchEvent = Extract<
  WorkflowEvent,
  | { event: "planning" }
  | { event: "plan_ready" }
  | { event: "stage_update" }
  | { event: "batch_progress" }
  | { event: "batch_started" }
  | { event: "item_started" }
  | { event: "item_progress" }
  | { event: "item_image_ready" }
  | { event: "item_agent_stream" }
  | { event: "item_approval_pending" }
  | { event: "item_done" }
  | { event: "item_error" }
  | { event: "cancelled" }
  | { event: "batch_done" }
  | { event: "error" }
>;

export type { PlanItem, ModPlan };

export class BatchSocket extends WorkflowSocketFacade<BatchEvent> {
  private errorHandler: ((data: BatchEvent) => void) | null = null;

  constructor() {
    super("/api/ws/batch");
  }

  override on<T extends BatchEvent["event"]>(event: T, handler: (data: Extract<BatchEvent, { event: T }>) => void) {
    super.on(event, handler);
    if (event === "error") {
      this.errorHandler = handler as (data: BatchEvent) => void;
    }
    return this;
  }

  override waitOpen(): Promise<void> {
    return super.waitOpen().then(() => {
      this.attachPersistentErrorHandlers((message) => {
        this.errorHandler?.({ event: "error", stage: "error", message });
      });
    });
  }
}
