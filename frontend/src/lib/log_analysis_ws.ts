import type { WorkflowLogChannel, WorkflowScope } from "../shared/types/workflow.ts";
import { WorkflowSocketFacade } from "../shared/ws/facade.ts";

export type LogAnalysisEvent =
  | { event: "stage_update"; stage: string; scope: WorkflowScope; message: string }
  | { event: "log_info"; stage: "log_info"; lines: number }
  | { event: "stream"; stage: "stream"; chunk: string; source?: string; channel?: WorkflowLogChannel; model?: string }
  | { event: "done"; stage: "done"; full: string }
  | { event: "error"; stage: "error"; message: string; code?: string; detail?: string; request_id?: string };

export class LogAnalysisSocket extends WorkflowSocketFacade<LogAnalysisEvent> {
  private errorHandler: ((data: LogAnalysisEvent) => void) | null = null;

  constructor() {
    super("/api/ws/analyze-log");
  }

  override on<T extends LogAnalysisEvent["event"]>(
    event: T,
    handler: (data: Extract<LogAnalysisEvent, { event: T }>) => void,
  ) {
    super.on(event, handler);
    if (event === "error") {
      this.errorHandler = handler as (data: LogAnalysisEvent) => void;
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
