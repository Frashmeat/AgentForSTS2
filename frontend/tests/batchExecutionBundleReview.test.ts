import test from "node:test";
import assert from "node:assert/strict";

import {
  batchWorkflowReducer,
  canProceedFromBundleReview,
  createInitialBatchRuntimeState,
  reconcileBundleDecisionRecord,
  resolveExecutionBundleKey,
  summarizeBundleDecisionProgress,
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

test("canProceedFromBundleReview allows accepted risk bundle to proceed", () => {
  const review = createReviewPayload({
    execution_plan: {
      strictness: "balanced",
      dependency_groups: [{ item_ids: ["item-1", "item-2"] }],
      execution_bundles: [
        {
          bundle_id: "bundle:shared",
          item_ids: ["item-1", "item-2"],
          status: "needs_confirmation",
          reason: "unclear coupling",
          risk_codes: ["unclear_coupling"],
        },
      ],
    },
  });

  assert.equal(canProceedFromBundleReview(review, { "bundle:shared": "accepted" }), true);
});

test("reconcileBundleDecisionRecord rebuilds unresolved state for non-clear bundles", () => {
  const review = createReviewPayload({
    execution_plan: {
      strictness: "balanced",
      dependency_groups: [{ item_ids: ["item-1", "item-2"] }],
      execution_bundles: [
        {
          bundle_id: "bundle:clear",
          item_ids: ["item-1"],
          status: "clear",
          reason: "single item",
          risk_codes: [],
        },
        {
          bundle_id: "bundle:risky",
          item_ids: ["item-2"],
          status: "needs_confirmation",
          reason: "unclear coupling",
          risk_codes: ["unclear_coupling"],
        },
      ],
    },
  });

  const decisions = reconcileBundleDecisionRecord(review, { "bundle:risky": "accepted" });

  assert.deepEqual(decisions, { "bundle:risky": "accepted" });
});

test("resolveExecutionBundleKey falls back to item ids when bundle_id is missing", () => {
  const key = resolveExecutionBundleKey({
    item_ids: ["item-1", "item-2"],
    status: "needs_confirmation",
    reason: "unclear coupling",
    risk_codes: ["unclear_coupling"],
  });

  assert.equal(key, "bundle:item-1::item-2");
});

test("summarizeBundleDecisionProgress counts blocking and accepted decisions separately", () => {
  const review = createReviewPayload({
    execution_plan: {
      strictness: "balanced",
      dependency_groups: [{ item_ids: ["item-1", "item-2", "item-3"] }],
      execution_bundles: [
        {
          bundle_id: "bundle:accepted",
          item_ids: ["item-1"],
          status: "needs_confirmation",
          reason: "unclear coupling",
          risk_codes: ["unclear_coupling"],
        },
        {
          bundle_id: "bundle:split",
          item_ids: ["item-2", "item-3"],
          status: "split_recommended",
          reason: "large bundle",
          risk_codes: ["bundle_size_threshold"],
        },
      ],
    },
  });

  const summary = summarizeBundleDecisionProgress(review, {
    "bundle:accepted": "accepted",
    "bundle:split": "split_requested",
  });

  assert.equal(summary.accepted, 1);
  assert.equal(summary.splitRequested, 1);
  assert.equal(summary.blocking, 1);
});
