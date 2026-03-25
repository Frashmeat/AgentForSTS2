import type { WorkflowEvent, ModPlan, PlanItem } from "../shared/types/workflow.ts";
import { WorkflowClient } from "../shared/ws/client.ts";

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
  | { event: "batch_done" }
  | { event: "error" }
>;

export type { PlanItem, ModPlan };

export class BatchSocket {
  private client: WorkflowClient;

  constructor() {
    this.client = new WorkflowClient("/api/ws/batch");
  }

  on<T extends BatchEvent["event"]>(
    event: T,
    handler: (data: Extract<BatchEvent, { event: T }>) => void
  ) {
    this.client.on(event, handler as (data: import("../shared/types/workflow.ts").WorkflowEvent) => void);
    return this;
  }

  send(data: object) {
    this.client.send(data);
  }

  waitOpen(): Promise<void> {
    return this.client.waitOpen();
  }

  close() {
    this.client.close();
  }
}
