import type { ApprovalRequest } from "../../shared/api/index.ts";
import type { BatchItemStateRecord } from "./state.ts";

export interface BatchApprovalSocketLike {
  send(data: object): void;
}

function isProceedableStatus(status: string): boolean {
  return status === "approved" || status === "succeeded";
}

export function canProceedBatchApproval(requests: ApprovalRequest[]): boolean {
  return requests.length === 0 || requests.every((request) => isProceedableStatus(request.status));
}

export function applyBatchApprovalUpdate(
  itemStates: BatchItemStateRecord,
  actionId: string,
  request: ApprovalRequest,
): BatchItemStateRecord {
  let changed = false;
  const next = Object.fromEntries(
    Object.entries(itemStates).map(([itemId, itemState]) => {
      if (!itemState.approvalRequests.some((candidate) => candidate.action_id === actionId)) {
        return [itemId, itemState];
      }

      changed = true;
      return [
        itemId,
        {
          ...itemState,
          approvalRequests: itemState.approvalRequests.map((candidate) =>
            candidate.action_id === actionId ? request : candidate,
          ),
        },
      ];
    }),
  ) as BatchItemStateRecord;

  return changed ? next : itemStates;
}

export function markBatchApprovalResuming(
  itemStates: BatchItemStateRecord,
  itemId: string,
): BatchItemStateRecord {
  const actionIds = new Set((itemStates[itemId]?.approvalRequests ?? []).map((request) => request.action_id));
  if (actionIds.size === 0) {
    return itemStates;
  }

  let changed = false;
  const next = Object.fromEntries(
    Object.entries(itemStates).map(([candidateId, itemState]) => {
      if (
        itemState.status !== "approval_pending" ||
        !itemState.approvalRequests.some((request) => actionIds.has(request.action_id))
      ) {
        return [candidateId, itemState];
      }

      changed = true;
      return [
        candidateId,
        {
          ...itemState,
          status: "code_generating",
        },
      ];
    }),
  ) as BatchItemStateRecord;

  return changed ? next : itemStates;
}

export function resumeBatchApprovalWorkflow(socket: BatchApprovalSocketLike, itemId: string): void {
  socket.send({ action: "approve_all", item_id: itemId });
  socket.send({ action: "resume", item_id: itemId });
}
