import test from "node:test";
import assert from "node:assert/strict";

import {
  batchWorkflowReducer,
  canProceedFromBundleReview,
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

test("review_bundles_confirmed moves runtime into executing stage", () => {
  const state = batchWorkflowReducer(
    batchWorkflowReducer(createInitialBatchRuntimeState(), {
      type: "plan_ready_received",
    }),
    {
      type: "review_items_confirmed",
    },
  );

  const next = batchWorkflowReducer(state, {
    type: "review_bundles_confirmed",
  });

  assert.equal(next.stage, "executing");
});

test("canProceedFromBundleReview rejects when a bundle still needs confirmation", () => {
  const review = createReviewPayload({
    execution_plan: {
      strictness: "balanced",
      dependency_groups: [{ item_ids: ["item-1", "item-2"] }],
      execution_bundles: [
        {
          item_ids: ["item-1", "item-2"],
          status: "needs_confirmation",
          reason: "unclear coupling",
          risk_codes: ["unclear_coupling"],
        },
      ],
    },
  });

  assert.equal(canProceedFromBundleReview(review), false);
});
