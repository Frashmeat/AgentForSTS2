// 从 view.tsx 抽出的纯函数 helpers；无 React/JSX 依赖。

import type { PlanItem } from "../../lib/batch_ws";
import type {
  ExecutionBundlePreview,
  ExecutionBundleRecommendedAction,
  ExecutionBundleRiskDetail,
  PlanReviewPayload,
} from "../../shared/types/workflow.ts";
import type { ReviewStrictness } from "./state.ts";
import { FALLBACK_BUNDLE_ACTIONS, FALLBACK_BUNDLE_RISK_DETAILS, type ReviewFeedback } from "./view-constants.ts";

export function normalizeReviewStrictness(value: unknown): ReviewStrictness {
  return value === "efficient" || value === "strict" ? value : "balanced";
}

export function readJsonStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function writeJsonStorage(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 忽略写入失败：localStorage 可能配额满或被禁用，与本组件状态无关
  }
}

export function writeTextStorage(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 同上
  }
}

export function resolvePlanFieldValue(item: PlanItem, field: string): unknown {
  return (item as unknown as Record<string, unknown>)[field];
}

export function hasMeaningfulPlanFieldValue(item: PlanItem, field: string): boolean {
  const value = resolvePlanFieldValue(item, field);
  if (Array.isArray(value)) {
    return value.some((entry) => typeof entry === "string" && entry.trim().length > 0);
  }
  if (typeof value === "string") {
    return value.trim().length > 0;
  }
  if (typeof value === "boolean") {
    return value;
  }
  return value !== null && value !== undefined;
}

export function canProceedFromEditedItemReview(review: PlanReviewPayload | null, items: PlanItem[]): boolean {
  if (!review) {
    return true;
  }

  return review.validation.items.every((reviewItem) => {
    if (reviewItem.status === "clear") {
      return true;
    }
    if (reviewItem.status === "invalid") {
      return false;
    }
    const item = items.find((candidate) => candidate.id === reviewItem.item_id);
    if (!item) {
      return false;
    }
    if (reviewItem.missing_fields.some((field) => !hasMeaningfulPlanFieldValue(item, field))) {
      return false;
    }
    return reviewItem.issues.every((issue) => !issue.field || hasMeaningfulPlanFieldValue(item, issue.field));
  });
}

export function summarizePlanReview(
  review: PlanReviewPayload,
  items: PlanItem[],
): { feedback: ReviewFeedback; focusItemId: string | null } {
  const totalItems = review.validation.items.length;
  const clearItems = review.validation.items.filter((item) => item.status === "clear").length;
  const firstBlockingItem = review.validation.items.find((item) => item.status !== "clear") ?? null;

  if (firstBlockingItem) {
    const itemName = items.find((item) => item.id === firstBlockingItem.item_id)?.name ?? firstBlockingItem.item_id;
    return {
      feedback: {
        tone: "warning",
        message: `复核完成：${clearItems}/${totalItems} 项可继续。已定位到 ${itemName}，请继续补充说明后再重新检查。`,
      },
      focusItemId: firstBlockingItem.item_id,
    };
  }

  const totalBundles = review.execution_plan.execution_bundles.length;
  const clearBundles = review.execution_plan.execution_bundles.filter((bundle) => bundle.status === "clear").length;
  if (clearBundles < totalBundles) {
    return {
      feedback: {
        tone: "warning",
        message: `Item 复核已通过，但执行策略仍有 ${totalBundles - clearBundles} 个 bundle 需要确认。`,
      },
      focusItemId: null,
    };
  }

  return {
    feedback: {
      tone: "success",
      message: "复核完成：当前计划已通过，可以进入下一步。",
    },
    focusItemId: null,
  };
}

export function getBundleRiskDetails(bundle: ExecutionBundlePreview): ExecutionBundleRiskDetail[] {
  if (bundle.risk_details && bundle.risk_details.length > 0) {
    return bundle.risk_details;
  }
  return bundle.risk_codes
    .map((riskCode) => FALLBACK_BUNDLE_RISK_DETAILS[riskCode])
    .filter((detail): detail is ExecutionBundleRiskDetail => Boolean(detail));
}

export function getBundleRecommendedActions(bundle: ExecutionBundlePreview): ExecutionBundleRecommendedAction[] {
  if (bundle.recommended_actions && bundle.recommended_actions.length > 0) {
    return bundle.recommended_actions;
  }
  return FALLBACK_BUNDLE_ACTIONS[bundle.status] ?? [];
}

export function getBundleBlockingReason(bundle: ExecutionBundlePreview): string {
  if (bundle.blocking_reason?.trim()) {
    return bundle.blocking_reason;
  }
  if (bundle.status === "split_recommended") {
    return "系统建议先拆分该 bundle，再进入执行阶段。";
  }
  if (bundle.status === "needs_confirmation") {
    return "系统认为该 bundle 可执行，但仍需要你显式确认是否接受当前分组。";
  }
  return "当前 bundle 可直接执行。";
}
