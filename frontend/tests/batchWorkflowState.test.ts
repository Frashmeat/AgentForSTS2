import test from "node:test";
import assert from "node:assert/strict";

import {
  batchWorkflowReducer,
  createInitialBatchRuntimeState,
} from "../src/features/batch-generation/state.ts";

test("planning_started resets runtime and enters planning stage", () => {
  const dirty = {
    ...createInitialBatchRuntimeState(),
    stage: "error" as const,
    batchLog: ["old"],
    workflowErrorMessage: "boom",
  };

  const next = batchWorkflowReducer(dirty, { type: "planning_started" });

  assert.equal(next.stage, "planning");
  assert.deepEqual(next.batchLog, []);
  assert.equal(next.workflowErrorMessage, null);
});

test("batch_started initializes items and selects first item", () => {
  const next = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "batch_started",
    items: [{ id: "a" }, { id: "b" }],
  });

  assert.equal(next.stage, "executing");
  assert.equal(next.activeItemId, "a");
  assert.equal(Object.keys(next.itemStates).length, 2);
});

test("item_stage_message updates item stage history", () => {
  const started = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "batch_started",
    items: [{ id: "a" }],
  });

  const next = batchWorkflowReducer(started, {
    type: "item_stage_message",
    itemId: "a",
    message: "正在生图",
  });

  assert.equal(next.itemStates["a"].currentStage, "正在生图");
  assert.deepEqual(next.itemStates["a"].stageHistory, ["正在生图"]);
});

test("item_image_ready writes image and keeps first active item", () => {
  const started = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "batch_started",
    items: [{ id: "a" }],
  });

  const next = batchWorkflowReducer(started, {
    type: "item_image_ready",
    itemId: "a",
    image: "img-b64",
    index: 0,
    prompt: "prompt",
  });

  assert.equal(next.activeItemId, "a");
  assert.equal(next.itemStates["a"].status, "awaiting_selection");
  assert.deepEqual(next.itemStates["a"].images, ["img-b64"]);
});

test("item approval pending stores approval data", () => {
  const started = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "batch_started",
    items: [{ id: "a" }],
  });

  const next = batchWorkflowReducer(started, {
    type: "item_approval_pending",
    itemId: "a",
    summary: "需要审批",
    requests: [
      {
        action_id: "req-1",
        kind: "shell_command",
        title: "run",
        reason: "because",
        risk_level: "medium",
        requires_approval: true,
        status: "pending",
        payload: {},
      },
    ],
  });

  assert.equal(next.itemStates["a"].status, "approval_pending");
  assert.equal(next.itemStates["a"].approvalSummary, "需要审批");
  assert.equal(next.itemStates["a"].approvalRequests.length, 1);
});

test("approval_request_updated marks item done when all requests succeeded", () => {
  const withApproval = batchWorkflowReducer(
    batchWorkflowReducer(createInitialBatchRuntimeState(), {
      type: "batch_started",
      items: [{ id: "a" }],
    }),
    {
      type: "item_approval_pending",
      itemId: "a",
      summary: "需要审批",
      requests: [
        {
          action_id: "req-1",
          kind: "shell_command",
          title: "run",
          reason: "because",
          risk_level: "medium",
          requires_approval: true,
          status: "pending",
          payload: {},
        },
      ],
    },
  );

  const next = batchWorkflowReducer(withApproval, {
    type: "approval_request_updated",
    actionId: "req-1",
    request: {
      action_id: "req-1",
      kind: "shell_command",
      title: "run",
      reason: "because",
      risk_level: "medium",
      requires_approval: true,
      status: "succeeded",
      payload: {},
    },
  });

  assert.equal(next.itemStates["a"].status, "done");
});

test("workflow_reset returns runtime defaults", () => {
  const dirty = {
    ...createInitialBatchRuntimeState(),
    stage: "done" as const,
    batchLog: ["done"],
    activeItemId: "a",
  };

  const next = batchWorkflowReducer(dirty, { type: "workflow_reset" });

  assert.deepEqual(next, createInitialBatchRuntimeState());
});

test("stage_set updates current batch stage", () => {
  const next = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "stage_set",
    stage: "executing",
  });

  assert.equal(next.stage, "executing");
});
