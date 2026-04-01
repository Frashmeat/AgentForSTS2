import test from "node:test";
import assert from "node:assert/strict";

import {
  createInitialSingleAssetWorkflowState,
  singleAssetWorkflowReducer,
} from "../src/features/single-asset/state.ts";

test("start action resets old workflow state and enters image generation for ai mode", () => {
  const dirty = {
    ...createInitialSingleAssetWorkflowState(),
    stage: "error" as const,
    images: ["old-image"],
    genLog: ["old log"],
    errorMsg: "failed",
    errorTrace: "trace",
  };

  const next = singleAssetWorkflowReducer(dirty, {
    type: "workflow_started",
    imageMode: "ai",
  });

  assert.equal(next.stage, "generating_image");
  assert.deepEqual(next.images, []);
  assert.deepEqual(next.genLog, []);
  assert.equal(next.errorMsg, null);
  assert.equal(next.errorTrace, null);
});

test("prompt preview writes prompt fields and switches to confirm prompt", () => {
  const next = singleAssetWorkflowReducer(createInitialSingleAssetWorkflowState(), {
    type: "prompt_preview_received",
    prompt: "draw a relic",
    negativePrompt: "blurry",
    fallbackWarning: "fallback",
  });

  assert.equal(next.stage, "confirm_prompt");
  assert.equal(next.promptPreview, "draw a relic");
  assert.equal(next.currentPrompt, "draw a relic");
  assert.equal(next.negativePrompt, "blurry");
  assert.equal(next.promptFallbackWarn, "fallback");
});

test("image ready appends image and enters pick image stage", () => {
  const next = singleAssetWorkflowReducer(createInitialSingleAssetWorkflowState(), {
    type: "image_ready_received",
    index: 0,
    image: "base64-image",
    prompt: "current prompt",
    batchOffset: 0,
  });

  assert.equal(next.stage, "pick_image");
  assert.deepEqual(next.images, ["base64-image"]);
  assert.equal(next.pendingSlots, 0);
  assert.equal(next.currentPrompt, "current prompt");
});

test("approval pending stores approval payload and enters approval state", () => {
  const next = singleAssetWorkflowReducer(createInitialSingleAssetWorkflowState(), {
    type: "approval_pending_received",
    summary: "需要审批",
    requests: [
      {
        action_id: "a1",
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

  assert.equal(next.stage, "approval_pending");
  assert.equal(next.approvalSummary, "需要审批");
  assert.equal(next.approvalRequests.length, 1);
  assert.match(next.agentLog.join(""), /等待用户审批/);
});

test("error action captures message and moves to error stage", () => {
  const next = singleAssetWorkflowReducer(createInitialSingleAssetWorkflowState(), {
    type: "workflow_failed",
    message: "boom",
    traceback: "traceback",
  });

  assert.equal(next.stage, "error");
  assert.equal(next.errorMsg, "boom");
  assert.equal(next.errorTrace, "traceback");
});

test("reset clears workflow state back to defaults", () => {
  const dirty = {
    ...createInitialSingleAssetWorkflowState(),
    stage: "done" as const,
    images: ["img"],
    approvalSummary: "审批",
    approvalRequests: [{ action_id: "a1" }],
    agentLog: ["done"],
  };

  const next = singleAssetWorkflowReducer(dirty, { type: "workflow_reset" });

  assert.deepEqual(next, createInitialSingleAssetWorkflowState());
});
