export type ExecutionGraphStatus =
  | "running"
  | "validating"
  | "repairing"
  | "pause_requested"
  | "paused"
  | "cancel_requested"
  | "cancelled"
  | "commit_prepared"
  | "commit_blocked"
  | "succeeded";

export interface ExecutionGraphView {
  executionGraphId: string;
  revision: number;
  status: ExecutionGraphStatus;
  activeRunId: string | null;
  previousRunId: string | null;
  completedNodes: number;
  totalNodes: number;
  currentNodeId: string | null;
  currentRoleId: string | null;
  failureCode: string | null;
  repairRound: number;
  feedbackPhase: "output_contract" | "generated_content" | null;
  canPause: boolean;
  canResume: boolean;
  canCancel: boolean;
}

export function isExecutionGraphView(value: unknown): value is ExecutionGraphView {
  if (!isRecord(value)) return false;
  return (
    typeof value.executionGraphId === "string" &&
    typeof value.revision === "number" &&
    Number.isInteger(value.revision) &&
    value.revision > 0 &&
    [
      "running",
      "validating",
      "repairing",
      "pause_requested",
      "paused",
      "cancel_requested",
      "cancelled",
      "commit_prepared",
      "commit_blocked",
      "succeeded",
    ].includes(String(value.status)) &&
    (value.activeRunId === null || typeof value.activeRunId === "string") &&
    (value.previousRunId === null || typeof value.previousRunId === "string") &&
    typeof value.completedNodes === "number" &&
    Number.isInteger(value.completedNodes) &&
    value.completedNodes >= 0 &&
    typeof value.totalNodes === "number" &&
    Number.isInteger(value.totalNodes) &&
    value.totalNodes >= 0 &&
    value.completedNodes <= value.totalNodes &&
    (value.currentNodeId === null || typeof value.currentNodeId === "string") &&
    (value.currentRoleId === null || typeof value.currentRoleId === "string") &&
    (value.failureCode === null || typeof value.failureCode === "string") &&
    typeof value.repairRound === "number" &&
    Number.isInteger(value.repairRound) &&
    value.repairRound >= 0 &&
    (value.feedbackPhase === null ||
      value.feedbackPhase === "output_contract" ||
      value.feedbackPhase === "generated_content") &&
    typeof value.canPause === "boolean" &&
    typeof value.canResume === "boolean" &&
    typeof value.canCancel === "boolean"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
