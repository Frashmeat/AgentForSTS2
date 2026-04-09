import test from "node:test";
import assert from "node:assert/strict";

import {
  batchWorkflowReducer,
  canProceedFromItemReview,
  createInitialBatchRuntimeState,
} from "../src/features/batch-generation/state.ts";
import type { PlanReviewPayload } from "../src/shared/types/workflow.ts";

function createReviewPayload(overrides: Partial<PlanReviewPayload> = {}): PlanReviewPayload {
  return {
    strictness: "balanced",
    validation: {
      strictness: "balanced",
      items: [
        {
          item_id: "item-1",
          status: "clear",
          issues: [],
          missing_fields: [],
          clarification_questions: [],
        },
      ],
    },
    execution_plan: {
      strictness: "balanced",
      dependency_groups: [{ item_ids: ["item-1"] }],
      execution_bundles: [
        {
          item_ids: ["item-1"],
          status: "clear",
          reason: "single item",
          risk_codes: [],
        },
      ],
    },
    ...overrides,
  };
}

test("plan_ready_received enters review_items stage", () => {
  const next = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "plan_ready_received",
  });

  assert.equal(next.stage, "review_items");
});

test("review_items_confirmed moves runtime into review_bundles stage", () => {
  const state = batchWorkflowReducer(createInitialBatchRuntimeState(), {
    type: "plan_ready_received",
  });

  const next = batchWorkflowReducer(state, {
    type: "review_items_confirmed",
  });

  assert.equal(next.stage, "review_bundles");
});

test("canProceedFromItemReview rejects when any item still needs input", () => {
  const review = createReviewPayload({
    validation: {
      strictness: "balanced",
      items: [
        {
          item_id: "item-1",
          status: "needs_user_input",
          issues: [],
          missing_fields: ["goal"],
          clarification_questions: ["请补充目标"],
        },
      ],
    },
  });

  assert.equal(canProceedFromItemReview(review), false);
});
