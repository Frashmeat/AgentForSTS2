import type { WorkflowScope } from "../shared/types/workflow.ts";
import { WorkflowSocketFacade } from "../shared/ws/facade.ts";

export type ModAnalysisEvent =
  | { event: "stage_update"; stage: string; scope: WorkflowScope; message: string }
  | { event: "scan_info"; stage: "scan_info"; files: number }
  | { event: "stream"; stage: "stream"; chunk: string }
  | { event: "done"; stage: "done"; full: string }
  | { event: "error"; stage: "error"; message: string; code?: string; detail?: string; request_id?: string };

export class ModAnalysisSocket extends WorkflowSocketFacade<ModAnalysisEvent> {
  private errorHandler: ((data: ModAnalysisEvent) => void) | null = null;

  constructor() {
    super("/api/ws/analyze-mod");
  }

  override on<T extends ModAnalysisEvent["event"]>(
    event: T,
    handler: (data: Extract<ModAnalysisEvent, { event: T }>) => void
  ) {
    super.on(event, handler);
    if (event === "error") {
      this.errorHandler = handler as (data: ModAnalysisEvent) => void;
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
