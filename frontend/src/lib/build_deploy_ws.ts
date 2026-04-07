import type { WorkflowLogChannel } from "../shared/types/workflow.ts";
import { WorkflowSocketFacade } from "../shared/ws/facade.ts";

export type BuildDeployEvent =
  | { event: "stream"; stage: "stream"; chunk: string; source?: string; channel?: WorkflowLogChannel; model?: string }
  | { event: "done"; stage: "done"; success: boolean; deployed_to?: string | null; files?: string[] }
  | { event: "error"; stage: "error"; message: string; code?: string; detail?: string; request_id?: string };

export class BuildDeploySocket extends WorkflowSocketFacade<BuildDeployEvent> {
  private errorHandler: ((data: BuildDeployEvent) => void) | null = null;

  constructor() {
    super("/api/ws/build-deploy");
  }

  override on<T extends BuildDeployEvent["event"]>(
    event: T,
    handler: (data: Extract<BuildDeployEvent, { event: T }>) => void
  ) {
    super.on(event, handler);
    if (event === "error") {
      this.errorHandler = handler as (data: BuildDeployEvent) => void;
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
