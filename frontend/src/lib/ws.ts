import type { WorkflowEvent } from "../shared/types/workflow.ts";
import { WorkflowSocketFacade } from "../shared/ws/facade.ts";

export type WsEvent = Extract<
  WorkflowEvent,
  | { event: "stage_update" }
  | { event: "progress" }
  | { event: "prompt_preview" }
  | { event: "image_ready" }
  | { event: "agent_stream" }
  | { event: "approval_pending" }
  | { event: "cancelled" }
  | { event: "done" }
  | { event: "error" }
>;

export class WorkflowSocket extends WorkflowSocketFacade<WsEvent> {
  private errorHandler: ((data: WsEvent) => void) | null = null;

  constructor() {
    super("/api/ws/create");
  }

  override on<T extends WsEvent["event"]>(
    event: T,
    handler: (data: Extract<WsEvent, { event: T }>) => void
  ) {
    super.on(event, handler);
    if (event === "error") {
      this.errorHandler = handler as (data: WsEvent) => void;
    }
    return this;
  }

  override waitOpen(): Promise<void> {
    return super.waitOpen().then(() => {
      this.client.attachPersistentErrorHandlers((message) => {
        this.errorHandler?.({ event: "error", stage: "error", message });
      });
    });
  }
}
