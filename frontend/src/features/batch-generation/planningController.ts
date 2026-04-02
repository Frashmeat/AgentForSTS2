import { BatchSocket, type ModPlan } from "../../lib/batch_ws.ts";
import { generateModPlan } from "../../shared/api/index.ts";

export interface BatchPlanningSocketLike {
  waitOpen(): Promise<void>;
  send(data: object): void;
  close(): void;
}

export interface BatchPlanningRuntime {
  closeSocket(): void;
  setSocket(socket: BatchPlanningSocketLike | null): void;
  clearProjectCreationFeedback(): void;
  setRestoredSnapshotMode(value: boolean): void;
  setRestoredApprovalRefreshPending(value: boolean): void;
  dispatchPlanningStarted(): void;
  clearPlan(): void;
  applyGeneratedPlan(plan: ModPlan): void;
  registerSocketHandlers(socket: BatchPlanningSocketLike): void;
  reportWorkflowError(message: string): void;
}

interface BatchPlanningDeps {
  createSocket(): BatchPlanningSocketLike;
  generateModPlan(requirements: string): Promise<ModPlan>;
}

function resolveErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createBatchPlanningController(
  runtime: BatchPlanningRuntime,
  deps: Partial<BatchPlanningDeps> = {},
) {
  const createSocket = deps.createSocket ?? (() => new BatchSocket());
  const requestGenerateModPlan = deps.generateModPlan ?? generateModPlan;

  function preparePlanning() {
    runtime.closeSocket();
    runtime.setSocket(null);
    runtime.clearProjectCreationFeedback();
    runtime.setRestoredSnapshotMode(false);
    runtime.setRestoredApprovalRefreshPending(false);
    runtime.dispatchPlanningStarted();
    runtime.clearPlan();
  }

  async function startSocketPlanning(requirements: string, projectRoot: string) {
    if (!requirements.trim()) {
      return;
    }

    preparePlanning();

    const socket = createSocket();
    runtime.setSocket(socket);
    runtime.registerSocketHandlers(socket);

    try {
      await socket.waitOpen();
    } catch (error) {
      runtime.reportWorkflowError(resolveErrorMessage(error));
      return;
    }

    socket.send({
      action: "start",
      requirements,
      project_root: projectRoot,
    });
  }

  async function startHttpPlanning(requirements: string, projectRoot: string) {
    if (!requirements.trim() || !projectRoot.trim()) {
      return;
    }

    preparePlanning();

    try {
      runtime.applyGeneratedPlan(await requestGenerateModPlan(requirements));
    } catch (error) {
      runtime.reportWorkflowError(resolveErrorMessage(error));
    }
  }

  return {
    startSocketPlanning,
    startHttpPlanning,
  };
}
