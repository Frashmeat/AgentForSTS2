// BatchModePage 的 Plan/Bundle 复核状态机封装。
// 收纳 reviewBusy/reviewError/reviewFeedback/reviewFocusItemId 与 7 个复核 handler。
// 暴露 setters 给外层（_registerBatchHandlers / reset / startPlanning）做即时清空。

import { useState, type Dispatch } from "react";

import { PlanItem } from "../../lib/batch_ws";
import { ModPlan } from "../../lib/batch_ws";
import { reviewModPlan } from "../../shared/api/workflow.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
import type { ReviewFeedback } from "./view-constants.ts";
import {
  canProceedFromBundleReview,
  reconcileBundleDecisionRecord,
  summarizeBundleDecisionProgress,
  type BatchRuntimeAction,
  type BundleDecisionRecord,
  type BundleDecisionStatus,
  type ReviewStrictness,
} from "./state.ts";
import {
  PLAN_BUNDLE_DECISIONS_STORAGE_KEY,
  PLAN_REVIEW_STORAGE_KEY,
  PLAN_REVIEW_STRICTNESS_STORAGE_KEY,
} from "./view-constants.ts";
import {
  canProceedFromEditedItemReview,
  normalizeReviewStrictness,
  summarizePlanReview,
  writeJsonStorage,
  writeTextStorage,
} from "./view-helpers.ts";

export interface UseBatchPlanReviewOptions {
  plan: ModPlan | null;
  editedItems: PlanItem[];
  planReview: PlanReviewPayload | null;
  bundleDecisions: BundleDecisionRecord;
  reviewStrictness: ReviewStrictness;
  dispatchRuntime: Dispatch<BatchRuntimeAction>;
  onProceedToExecution: () => void;
}

export interface UseBatchPlanReviewResult {
  reviewBusy: boolean;
  reviewError: string | null;
  reviewFeedback: ReviewFeedback | null;
  reviewFocusItemId: string | null;
  setReviewError: (value: string | null) => void;
  setReviewFeedback: (value: ReviewFeedback | null) => void;
  setReviewFocusItemId: (value: string | null) => void;
  refreshPlanReview: (
    items?: PlanItem[],
    strictness?: ReviewStrictness,
    decisions?: BundleDecisionRecord,
  ) => Promise<PlanReviewPayload | null>;
  handleConfirmItemsReview: () => Promise<void>;
  handleConfirmBundleReview: () => Promise<void>;
  handleReviewStrictnessChange: (next: ReviewStrictness) => void;
  handleBundleDecisionChange: (bundleKey: string, decision: BundleDecisionStatus) => void;
  handleBundleSplitRequest: (bundleKey: string) => Promise<void>;
  handleBundleReturnToItems: (bundleKey: string, itemIds: string[]) => void;
}

export function useBatchPlanReview({
  plan,
  editedItems,
  planReview,
  bundleDecisions,
  reviewStrictness,
  dispatchRuntime,
  onProceedToExecution,
}: UseBatchPlanReviewOptions): UseBatchPlanReviewResult {
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [reviewFeedback, setReviewFeedback] = useState<ReviewFeedback | null>(null);
  const [reviewFocusItemId, setReviewFocusItemId] = useState<string | null>(null);

  async function refreshPlanReview(
    items: PlanItem[] = editedItems,
    strictness: ReviewStrictness = reviewStrictness,
    decisions: BundleDecisionRecord = bundleDecisions,
  ): Promise<PlanReviewPayload | null> {
    if (!plan) {
      return null;
    }
    setReviewBusy(true);
    setReviewError(null);
    setReviewFeedback({ tone: "info", message: "正在重新检查当前计划..." });
    try {
      const review = await reviewModPlan({
        plan: { ...plan, items },
        strictness,
        bundle_decisions: decisions,
      });
      const nextDecisions = reconcileBundleDecisionRecord(review);
      dispatchRuntime({ type: "review_updated", review, decisions: nextDecisions });
      const summary = summarizePlanReview(review, items);
      setReviewFeedback(summary.feedback);
      setReviewFocusItemId(summary.focusItemId);
      writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, review);
      writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
      writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, normalizeReviewStrictness(review.strictness));
      return review;
    } catch (error) {
      setReviewError(resolveErrorMessage(error));
      setReviewFeedback(null);
      return null;
    } finally {
      setReviewBusy(false);
    }
  }

  async function handleConfirmItemsReview() {
    const review = await refreshPlanReview();
    if (!review) {
      return;
    }
    if (canProceedFromEditedItemReview(review, editedItems)) {
      setReviewFeedback({ tone: "success", message: "Item 复核已通过，进入执行策略决策。" });
      dispatchRuntime({ type: "review_items_confirmed" });
    }
  }

  async function handleConfirmBundleReview() {
    const review = planReview ?? (await refreshPlanReview());
    if (!review) {
      return;
    }
    if (!canProceedFromEditedItemReview(review, editedItems)) {
      setReviewFeedback({ tone: "warning", message: "仍有 item 说明未补齐，已返回 Item 复核阶段。" });
      dispatchRuntime({ type: "stage_set", stage: "review_items" });
      return;
    }
    if (!canProceedFromBundleReview(review, bundleDecisions)) {
      const progress = summarizeBundleDecisionProgress(review, bundleDecisions);
      const pendingParts = [
        progress.unresolved > 0 ? `${progress.unresolved} 个 bundle 待决策` : null,
        progress.splitRequested > 0 ? `${progress.splitRequested} 个 bundle 已要求拆分但尚未完成重算` : null,
        progress.needsItemRevision > 0 ? `${progress.needsItemRevision} 个 bundle 仍需回到 Item 复核` : null,
      ].filter(Boolean);
      setReviewFeedback({
        tone: "warning",
        message: `执行策略仍需处理：${pendingParts.join("，")}。`,
      });
      dispatchRuntime({ type: "stage_set", stage: "review_bundles" });
      return;
    }
    setReviewFeedback({ tone: "success", message: "执行策略复核通过，开始进入执行阶段。" });
    onProceedToExecution();
  }

  function handleReviewStrictnessChange(nextStrictness: ReviewStrictness) {
    dispatchRuntime({ type: "review_strictness_set", strictness: nextStrictness });
    writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, nextStrictness);
    if (plan) {
      void refreshPlanReview(editedItems, nextStrictness, {});
    }
  }

  function handleBundleDecisionChange(bundleKey: string, decision: BundleDecisionStatus) {
    const next = { ...bundleDecisions, [bundleKey]: decision };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: next });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, next);
  }

  async function handleBundleSplitRequest(bundleKey: string) {
    const nextDecisions = { ...bundleDecisions, [bundleKey]: "split_requested" as const };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: nextDecisions });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
    setReviewFeedback({ tone: "info", message: "已记录拆分请求，正在按更保守口径重算 bundle..." });
    const review = await refreshPlanReview(editedItems, reviewStrictness, nextDecisions);
    if (!review) {
      return;
    }
    const progress = summarizeBundleDecisionProgress(review, reconcileBundleDecisionRecord(review));
    setReviewFeedback({
      tone: progress.blocking === 0 ? "success" : "warning",
      message:
        progress.blocking === 0
          ? "已按拆分请求重算执行策略，当前可以直接进入执行。"
          : "已按拆分请求重算执行策略，请继续处理剩余 bundle 决策。",
    });
  }

  function handleBundleReturnToItems(bundleKey: string, itemIds: string[]) {
    const next = { ...bundleDecisions, [bundleKey]: "needs_item_revision" as const };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: next });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, next);
    setReviewFocusItemId(itemIds[0] ?? null);
    setReviewFeedback({
      tone: "warning",
      message: "已返回 Item 复核阶段，请补充依赖原因、范围边界或验收说明后重新检查。",
    });
    dispatchRuntime({ type: "stage_set", stage: "review_items" });
  }

  return {
    reviewBusy,
    reviewError,
    reviewFeedback,
    reviewFocusItemId,
    setReviewError,
    setReviewFeedback,
    setReviewFocusItemId,
    refreshPlanReview,
    handleConfirmItemsReview,
    handleConfirmBundleReview,
    handleReviewStrictnessChange,
    handleBundleDecisionChange,
    handleBundleSplitRequest,
    handleBundleReturnToItems,
  };
}
