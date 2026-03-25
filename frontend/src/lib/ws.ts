import type { WorkflowEvent } from "../shared/types/workflow.ts";
import { WorkflowClient } from "../shared/ws/client.ts";

export type WsEvent = Extract<
  WorkflowEvent,
  | { event: "stage_update" }
  | { event: "progress" }
  | { event: "prompt_preview" }
  | { event: "image_ready" }
  | { event: "agent_stream" }
  | { event: "approval_pending" }
  | { event: "done" }
  | { event: "error" }
>;

export class WorkflowSocket {
  private client: WorkflowClient;
  private errorHandler: ((data: WsEvent) => void) | null = null;

  constructor() {
    this.client = new WorkflowClient("/api/ws/create");
  }

  on(event: WsEvent["event"], handler: (data: WsEvent) => void) {
    this.client.on(event, handler as (data: import("../shared/types/workflow.ts").WorkflowEvent) => void);
    if (event === "error") {
      this.errorHandler = handler;
    }
    return this;
  }

  send(data: object) {
    this.client.send(data);
  }

  waitOpen(): Promise<void> {
    return this.client.waitOpen().then(() => {
      this.client.attachPersistentErrorHandlers((message) => {
        this.errorHandler?.({ event: "error", stage: "error", message });
      });
    });
  }

  close() {
    this.client.close();
  }
}
