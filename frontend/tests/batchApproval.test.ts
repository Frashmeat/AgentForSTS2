import test from "node:test";
import assert from "node:assert/strict";

import type { ApprovalRequest } from "../src/shared/api/index.ts";
import {
  applyBatchApprovalUpdate,
  canProceedBatchApproval,
  markBatchApprovalResuming,
  resumeBatchApprovalWorkflow,
} from "../src/features/batch-generation/approval.ts";
import { createDefaultBatchItemState, type BatchItemStateRecord } from "../src/features/batch-generation/state.ts";

function createApproval(overrides: Partial<ApprovalRequest> = {}): ApprovalRequest {
  return {
    action_id: "req-1",
    kind: "shell_command",
    title: "run build",
    reason: "verify",
    risk_level: "medium",
    requires_approval: true,
    status: "pending",
    payload: {},
    ...overrides,
  };
}

test("canProceedBatchApproval requires every approval to leave pending state", () => {
  assert.equal(canProceedBatchApproval([]), false);
  assert.equal(canProceedBatchApproval([createApproval()]), false);
  assert.equal(
    canProceedBatchApproval([
      createApproval({ status: "approved" }),
      createApproval({ action_id: "req-2", status: "succeeded" }),
    ]),
    true,
  );
});

test("applyBatchApprovalUpdate refreshes all items sharing the same approval request", () => {
  const itemStates: BatchItemStateRecord = {
    "item-1": {
      ...createDefaultBatchItemState(),
      status: "approval_pending",
      approvalRequests: [createApproval()],
    },
    "item-2": {
      ...createDefaultBatchItemState(),
      status: "approval_pending",
      approvalRequests: [createApproval()],
    },
  };

  const next = applyBatchApprovalUpdate(itemStates, "req-1", createApproval({ status: "approved" }));

  assert.equal(next["item-1"].approvalRequests[0].status, "approved");
  assert.equal(next["item-2"].approvalRequests[0].status, "approved");
  assert.equal(next["item-1"].status, "approval_pending");
  assert.equal(next["item-2"].status, "approval_pending");
});

test("markBatchApprovalResuming moves the whole approval group into code generation", () => {
  const itemStates: BatchItemStateRecord = {
    "item-1": {
      ...createDefaultBatchItemState(),
      status: "approval_pending",
      approvalRequests: [createApproval()],
    },
    "item-2": {
      ...createDefaultBatchItemState(),
      status: "approval_pending",
      approvalRequests: [createApproval()],
    },
    "item-3": {
      ...createDefaultBatchItemState(),
      status: "approval_pending",
      approvalRequests: [createApproval({ action_id: "req-2" })],
    },
  };

  const next = markBatchApprovalResuming(itemStates, "item-1");

  assert.equal(next["item-1"].status, "code_generating");
  assert.equal(next["item-2"].status, "code_generating");
  assert.equal(next["item-3"].status, "approval_pending");
});

test("resumeBatchApprovalWorkflow sends approve_all then resume control messages", () => {
  const sent: object[] = [];
  resumeBatchApprovalWorkflow(
    {
      send(payload) {
        sent.push(payload);
      },
    },
    "item-1",
  );

  assert.deepEqual(sent, [
    { action: "approve_all", item_id: "item-1" },
    { action: "resume", item_id: "item-1" },
  ]);
});
