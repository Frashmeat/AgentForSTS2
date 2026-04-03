import test from "node:test";
import assert from "node:assert/strict";

import type { ApprovalRequest } from "../src/shared/api/index.ts";
import {
  BATCH_RUNTIME_SNAPSHOT_KEY,
  createRetryableBatchItemState,
  loadBatchRuntimeSnapshot,
  refreshRecoveredBatchApprovals,
  restoreBatchRuntimeSnapshot,
  saveBatchRuntimeSnapshot,
  serializeBatchRuntimeSnapshot,
} from "../src/features/batch-generation/recovery.ts";
import {
  createDefaultBatchItemState,
  createInitialBatchRuntimeState,
  type BatchRuntimeState,
} from "../src/features/batch-generation/state.ts";

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

function createStorageStub() {
  const store = new Map<string, string>();
  return {
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, value);
    },
    removeItem(key: string) {
      store.delete(key);
    },
  };
}

test("serialize and restore batch snapshot preserves recoverable runtime state", () => {
  const runtime: BatchRuntimeState = {
    ...createInitialBatchRuntimeState(),
    stage: "executing",
    activeItemId: "item-1",
    currentBatchStage: "正在执行",
    batchStageHistory: ["准备", "正在执行"],
    batchResult: { success: 1, error: 0 },
    approvalBusyActionId: "req-1",
    itemStates: {
      "item-1": {
        ...createDefaultBatchItemState(),
        status: "approval_pending",
        currentStage: "等待审批",
        stageHistory: ["开始", "等待审批"],
        progress: ["p1", "p2"],
        agentLog: ["a1"],
        currentPrompt: "prompt",
        approvalSummary: "需要审批",
        approvalRequests: [createApproval()],
        error: "old error",
        errorTrace: "traceback",
      },
    },
  };

  const snapshot = serializeBatchRuntimeSnapshot(runtime);
  const restored = restoreBatchRuntimeSnapshot(snapshot);

  assert.equal(snapshot.stage, "executing");
  assert.equal("approvalBusyActionId" in snapshot, false);
  assert.equal(restored?.stage, "executing");
  assert.equal(restored?.activeItemId, "item-1");
  assert.equal(restored?.itemStates["item-1"].approvalSummary, "需要审批");
  assert.equal(restored?.itemStates["item-1"].approvalRequests[0].action_id, "req-1");
});

test("saveBatchRuntimeSnapshot and loadBatchRuntimeSnapshot round-trip storage", () => {
  const storage = createStorageStub();
  const runtime: BatchRuntimeState = {
    ...createInitialBatchRuntimeState(),
    stage: "done",
    batchResult: { success: 2, error: 1 },
    itemStates: {
      "item-1": {
        ...createDefaultBatchItemState(),
        status: "done",
      },
    },
  };

  saveBatchRuntimeSnapshot(storage, runtime);
  const restored = loadBatchRuntimeSnapshot(storage);

  assert.equal(storage.getItem(BATCH_RUNTIME_SNAPSHOT_KEY) !== null, true);
  assert.equal(restored?.stage, "done");
  assert.equal(restored?.batchResult?.success, 2);
});

test("refreshRecoveredBatchApprovals replaces stale approval requests with backend truth", async () => {
  const runtime: BatchRuntimeState = {
    ...createInitialBatchRuntimeState(),
    stage: "executing",
    itemStates: {
      "item-1": {
        ...createDefaultBatchItemState(),
        status: "approval_pending",
        approvalSummary: "等待审批",
        approvalRequests: [createApproval()],
      },
    },
  };

  const refreshed = await refreshRecoveredBatchApprovals(runtime, async (actionId) =>
    createApproval({ action_id: actionId, status: "approved", source_backend: "codex" }),
  );

  assert.equal(refreshed.itemStates["item-1"].approvalRequests[0].status, "approved");
  assert.equal(refreshed.itemStates["item-1"].approvalRequests[0].source_backend, "codex");
  assert.equal(refreshed.itemStates["item-1"].status, "approval_pending");
});

test("refreshRecoveredBatchApprovals keeps item pending approval until workflow resumes", async () => {
  const runtime: BatchRuntimeState = {
    ...createInitialBatchRuntimeState(),
    stage: "executing",
    itemStates: {
      "item-1": {
        ...createDefaultBatchItemState(),
        status: "approval_pending",
        approvalSummary: "等待审批",
        approvalRequests: [createApproval()],
      },
    },
  };

  const refreshed = await refreshRecoveredBatchApprovals(runtime, async (actionId) =>
    createApproval({ action_id: actionId, status: "succeeded" }),
  );

  assert.equal(refreshed.itemStates["item-1"].status, "approval_pending");
  assert.equal(refreshed.itemStates["item-1"].approvalRequests[0].status, "succeeded");
});

test("createRetryableBatchItemState clears stale fields before retry", () => {
  const next = createRetryableBatchItemState({
    ...createDefaultBatchItemState(),
    status: "error",
    currentStage: "失败",
    stageHistory: ["开始", "失败"],
    progress: ["old progress"],
    images: ["old-image"],
    agentLog: ["old log"],
    error: "boom",
    errorTrace: "trace",
    currentPrompt: "keep?",
    showMorePrompt: true,
    approvalSummary: "需要审批",
    approvalRequests: [createApproval()],
  });

  assert.equal(next.status, "img_generating");
  assert.equal(next.currentStage, null);
  assert.deepEqual(next.progress, []);
  assert.deepEqual(next.images, []);
  assert.deepEqual(next.agentLog, []);
  assert.equal(next.error, null);
  assert.equal(next.errorTrace, null);
  assert.equal(next.approvalSummary, "");
  assert.deepEqual(next.approvalRequests, []);
  assert.equal(next.showMorePrompt, false);
});
