// 执行策略 bundle 决策视图：突出当前阶段、首要阻塞原因和下一步主动作。
// 从 view.tsx 抽出，纯 props-driven。

import type { PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import type { ExecutionBundlePreview, PlanReviewPayload } from "../../shared/types/workflow.ts";
import { ReviewFeedbackBanner, ReviewNotice, ReviewStatusBadge, ReviewStrictnessSelector } from "./ReviewBadges.tsx";
import {
  canProceedFromBundleReview,
  resolveExecutionBundleKey,
  summarizeBundleDecisionProgress,
  type BundleDecisionRecord,
  type BundleDecisionStatus,
  type ReviewStrictness,
} from "./state.ts";
import { BUNDLE_DECISION_LABELS, BUNDLE_DECISION_TONES, type ReviewFeedback } from "./view-constants.ts";
import { getBundleBlockingReason, getBundleRecommendedActions, getBundleRiskDetails } from "./view-helpers.ts";

type FocusAction =
  | "confirm"
  | "refresh"
  | "split"
  | "accept"
  | "revise"
  | "none";

interface FocusBundle {
  bundle: ExecutionBundlePreview;
  bundleKey: string;
  index: number;
  decision: BundleDecisionStatus;
}

function resolveFocusAction(
  canProceed: boolean,
  focus: FocusBundle | null,
): { action: FocusAction; label: string; note: string } {
  if (canProceed) {
    return {
      action: "confirm",
      label: "确认执行策略，开始执行",
      note: "点击后会锁定当前执行策略，并进入真正执行阶段。",
    };
  }
  if (!focus) {
    return {
      action: "refresh",
      label: "重新检查当前计划",
      note: "当前没有可处理的 bundle 结果，先重新检查一次。",
    };
  }
  if (focus.decision === "split_requested") {
    return {
      action: "refresh",
      label: "重新检查计划",
      note: "已记录拆分请求，需要重算后确认是否还有阻塞。",
    };
  }
  if (focus.decision === "needs_item_revision") {
    return {
      action: "revise",
      label: "返回补充 Item",
      note: "补充目标、依赖、范围或验收说明后再重新检查。",
    };
  }

  const recommendedActions = getBundleRecommendedActions(focus.bundle);
  if (recommendedActions.some((recommendedAction) => recommendedAction.action === "split_bundle")) {
    return {
      action: "split",
      label: "拆分并重新检查",
      note: "点击后会要求系统拆分当前 bundle，并重新计算阻塞数量。",
    };
  }
  if (recommendedActions.some((recommendedAction) => recommendedAction.action === "accept_bundle")) {
    return {
      action: "accept",
      label: "接受当前分组",
      note: "你确认这些 Item 应作为一个 bundle 联合执行。",
    };
  }
  if (recommendedActions.some((recommendedAction) => recommendedAction.action === "revise_items")) {
    return {
      action: "revise",
      label: "返回补充 Item",
      note: "当前信息不足，先补充 Item 说明再重新检查。",
    };
  }
  return {
    action: "refresh",
    label: "重新检查当前计划",
    note: "没有明确推荐动作，先重新检查以刷新判断。",
  };
}

export function ReviewBundles({
  items,
  review,
  reviewStrictness,
  reviewBusy,
  reviewError,
  reviewFeedback,
  bundleDecisions,
  onBack,
  onRefreshReview,
  onStrictnessChange,
  onBundleDecisionChange,
  onBundleSplitRequest,
  onBundleReturnToItems,
  onConfirm,
  onReset,
}: {
  items: PlanItem[];
  review: PlanReviewPayload | null;
  reviewStrictness: ReviewStrictness;
  reviewBusy: boolean;
  reviewError: string | null;
  reviewFeedback: ReviewFeedback | null;
  bundleDecisions: BundleDecisionRecord;
  onBack: () => void;
  onRefreshReview: () => void;
  onStrictnessChange: (value: ReviewStrictness) => void;
  onBundleDecisionChange: (bundleKey: string, decision: BundleDecisionStatus) => void;
  onBundleSplitRequest: (bundleKey: string) => void;
  onBundleReturnToItems: (bundleKey: string, itemIds: string[]) => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  const itemNameMap = new Map(items.map((item) => [item.id, item.name]));
  const dependencyGroups = review?.execution_plan.dependency_groups ?? [];
  const executionBundles = review?.execution_plan.execution_bundles ?? [];
  const progress = summarizeBundleDecisionProgress(review, bundleDecisions);
  const canProceed = canProceedFromBundleReview(review, bundleDecisions);
  const focusBundle: FocusBundle | null =
    executionBundles
      .map((bundle, index): FocusBundle | null => {
        if (bundle.status === "clear") {
          return null;
        }
        const bundleKey = resolveExecutionBundleKey(bundle, index);
        const decision = bundleDecisions[bundleKey] ?? "unresolved";
        if (decision === "accepted") {
          return null;
        }
        return { bundle, bundleKey, index, decision };
      })
      .find((bundle): bundle is FocusBundle => bundle !== null) ?? null;
  const focusAction = resolveFocusAction(canProceed, focusBundle);
  const focusItemNames =
    focusBundle?.bundle.item_ids.map((itemId) => itemNameMap.get(itemId) ?? itemId).join(" / ") ?? "";
  const headline = canProceed
    ? "执行策略已处理，可以开始执行"
    : progress.blocking > 0
      ? "计划暂时不能执行"
      : "等待执行策略检查结果";
  const summary = canProceed
    ? "当前没有阻塞 bundle。确认后会进入真正执行阶段。"
    : progress.blocking > 0
      ? `当前计划被 ${progress.blocking} 个阻塞 bundle 卡住。先处理主动作，处理完成后才能继续执行。`
      : "还没有可用的 bundle 检查结果，请先重新检查当前计划。";
  const mainReason = focusBundle
    ? getBundleBlockingReason(focusBundle.bundle)
    : canProceed
      ? "所有 bundle 都已是可执行状态，或已经被你明确接受。"
      : "系统还没有生成执行策略判断。";

  function runFocusAction() {
    if (reviewBusy) {
      return;
    }
    if (focusAction.action === "confirm") {
      onConfirm();
      return;
    }
    if (focusAction.action === "refresh") {
      onRefreshReview();
      return;
    }
    if (!focusBundle) {
      return;
    }
    if (focusAction.action === "split") {
      onBundleSplitRequest(focusBundle.bundleKey);
      return;
    }
    if (focusAction.action === "accept") {
      onBundleDecisionChange(focusBundle.bundleKey, "accepted");
      return;
    }
    if (focusAction.action === "revise") {
      onBundleReturnToItems(focusBundle.bundleKey, focusBundle.bundle.item_ids);
    }
  }

  return (
    <div className="space-y-4">
      <div className="workspace-surface rounded-2xl p-5 space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-violet-500">Step 2 / 2</p>
            <h2 className="font-bold text-slate-800">执行策略决策</h2>
            <p className="mt-0.5 text-xs text-slate-500">
              先处理阻塞 bundle，再确认执行策略。页面只突出当前最重要的一步。
            </p>
          </div>
          <span className="rounded-full border border-violet-200 bg-violet-50 px-2 py-0.5 text-xs font-medium text-violet-700">
            {progress.clear + progress.accepted}/{executionBundles.length || 0} 个 bundle 已具备执行条件
          </span>
        </div>

        <ReviewStrictnessSelector value={reviewStrictness} disabled={reviewBusy} onChange={onStrictnessChange} />
        <ReviewFeedbackBanner feedback={reviewFeedback} />
        <ReviewNotice message={reviewError} />

        <section className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm">
          <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_280px] lg:items-center">
            <div>
              <span
                className={cn(
                  "inline-flex rounded-full px-2.5 py-1 text-xs font-bold",
                  canProceed ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700",
                )}
              >
                当前主要信息
              </span>
              <h3 className="mt-3 text-2xl font-bold text-slate-900">{headline}</h3>
              <p className="mt-2 text-sm leading-6 text-slate-600">{summary}</p>
              <div className="mt-4 rounded-xl border border-amber-200 bg-amber-50 px-4 py-3">
                <p className="text-xs font-semibold text-amber-800">为什么卡住</p>
                <p className="mt-1 text-sm leading-6 text-amber-900">{mainReason}</p>
                {focusBundle ? <p className="mt-1 text-xs text-amber-800">当前处理：{focusItemNames}</p> : null}
              </div>
            </div>
            <div className="space-y-3">
              <button
                type="button"
                onClick={runFocusAction}
                disabled={reviewBusy || focusAction.action === "none"}
                className="w-full rounded-xl bg-violet-700 px-4 py-4 text-base font-bold text-white transition-colors hover:bg-violet-800 disabled:opacity-60"
              >
                {reviewBusy ? "处理中..." : focusAction.label}
              </button>
              <p className="text-center text-xs leading-5 text-slate-500">{focusAction.note}</p>
              {!canProceed && focusBundle ? (
                <div className="grid grid-cols-2 gap-2">
                  <button
                    type="button"
                    onClick={() => onBundleDecisionChange(focusBundle.bundleKey, "accepted")}
                    className="rounded-lg border border-slate-200 px-3 py-2 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-50"
                  >
                    仍按当前分组继续
                  </button>
                  <button
                    type="button"
                    onClick={() => onBundleReturnToItems(focusBundle.bundleKey, focusBundle.bundle.item_ids)}
                    className="rounded-lg border border-slate-200 px-3 py-2 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-50"
                  >
                    返回补充 Item
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </section>

        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">待你确认</p>
            <p className="mt-1 text-2xl font-semibold text-slate-800">{progress.unresolved}</p>
          </div>
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">已接受当前分组</p>
            <p className="mt-1 text-2xl font-semibold text-green-700">{progress.accepted}</p>
          </div>
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">仍阻塞执行</p>
            <p className="mt-1 text-2xl font-semibold text-amber-700">{progress.blocking}</p>
          </div>
        </div>

        <details className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
          <summary className="cursor-pointer text-sm font-medium text-slate-700">查看依赖分组和全部 bundle 详情</summary>
          <div className="mt-3 space-y-4">
            <div>
              <p className="text-xs text-slate-500">
                依赖分组是依赖关系视角；执行 Bundle 是系统综合耦合度和风险后给出的执行视角。
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                {dependencyGroups.length === 0 && (
                  <span className="text-xs text-slate-400">暂无分组数据，先重新检查一次。</span>
                )}
                {dependencyGroups.map((group, index) => (
                  <span
                    key={`group-${index}`}
                    className="rounded-full border border-slate-200 bg-white px-3 py-1 text-xs text-slate-600"
                  >
                    G{index + 1}: {group.item_ids.map((itemId) => itemNameMap.get(itemId) ?? itemId).join(" / ")}
                  </span>
                ))}
              </div>
            </div>

            <div className="space-y-3">
              {executionBundles.length === 0 && (
                <div className="rounded-xl border border-dashed border-slate-200 bg-white px-4 py-6 text-center text-sm text-slate-400">
                  还没有 bundle 评审结果，点击"重新检查当前计划"即可生成。
                </div>
              )}
              {executionBundles.map((bundle, index) => {
                const bundleKey = resolveExecutionBundleKey(bundle, index);
                const decision: BundleDecisionStatus =
                  bundle.status === "clear" ? "accepted" : (bundleDecisions[bundleKey] ?? "unresolved");
                const riskDetails = getBundleRiskDetails(bundle);
                const recommendedActions = getBundleRecommendedActions(bundle);

                return (
                  <div key={`bundle-${index}`} className="space-y-3 rounded-xl border border-slate-200 bg-white px-4 py-4">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <p className="text-sm font-semibold text-slate-800">执行 Bundle {index + 1}</p>
                        <p className="mt-1 text-xs text-slate-500">
                          {bundle.item_ids.map((itemId) => itemNameMap.get(itemId) ?? itemId).join(" / ")}
                        </p>
                      </div>
                      <div className="flex flex-wrap items-center justify-end gap-2">
                        <ReviewStatusBadge status={bundle.status} kind="bundle" />
                        <span
                          className={cn(
                            "rounded-full border px-2 py-0.5 text-xs font-medium",
                            BUNDLE_DECISION_TONES[decision],
                          )}
                        >
                          {BUNDLE_DECISION_LABELS[decision]}
                        </span>
                      </div>
                    </div>

                    <div className="grid gap-3 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                      <div className="rounded-lg bg-slate-50 px-3 py-3">
                        <p className="text-xs font-medium text-slate-500">分组理由</p>
                        <p className="mt-1 text-sm text-slate-700">{bundle.reason}</p>
                      </div>
                      <div className="rounded-lg bg-slate-50 px-3 py-3">
                        <p className="text-xs font-medium text-slate-500">风险标记</p>
                        <div className="mt-1 flex flex-wrap gap-2">
                          {bundle.risk_codes.length === 0 && <span className="text-sm text-green-700">无</span>}
                          {bundle.risk_codes.map((riskCode) => (
                            <span key={riskCode} className="rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700">
                              {riskCode}
                            </span>
                          ))}
                        </div>
                      </div>
                    </div>

                    <div className="rounded-lg bg-slate-50 px-3 py-3">
                      <p className="text-xs font-medium text-slate-500">阻塞原因</p>
                      <p className="mt-1 text-sm text-slate-700">{getBundleBlockingReason(bundle)}</p>
                    </div>

                    {riskDetails.length > 0 && (
                      <div className="space-y-2">
                        <p className="text-xs font-medium text-slate-500">风险解释</p>
                        {riskDetails.map((detail) => (
                          <div
                            key={`${bundleKey}-${detail.code}`}
                            className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-3"
                          >
                            <div className="flex items-center gap-2">
                              <span className="rounded-full bg-amber-100 px-2 py-0.5 text-[11px] font-medium text-amber-800">
                                {detail.title}
                              </span>
                              <span className="text-[11px] text-slate-400">{detail.code}</span>
                            </div>
                            <p className="mt-2 text-sm text-slate-700">{detail.summary}</p>
                            {detail.impact ? (
                              <p className="mt-1 text-xs text-slate-500">影响：{detail.impact}</p>
                            ) : null}
                            <p className="mt-2 text-xs text-slate-600">建议：{detail.recommendation}</p>
                          </div>
                        ))}
                      </div>
                    )}

                    {recommendedActions.length > 0 && (
                      <div className="grid gap-2 md:grid-cols-3">
                        {recommendedActions.map((action) => (
                          <div key={`${bundleKey}-${action.action}`} className="rounded-lg bg-slate-50 px-3 py-2">
                            <p className="text-xs font-medium text-slate-700">{action.label}</p>
                            <p className="mt-1 text-xs text-slate-500">{action.description}</p>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </details>

        <div className="mt-4 flex flex-wrap justify-between gap-2">
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={onBack}
              className="rounded-lg border border-slate-200 px-4 py-2.5 text-sm text-slate-600 transition-colors hover:text-slate-800"
            >
              返回 Item 复核
            </button>
            <button
              type="button"
              onClick={onRefreshReview}
              disabled={reviewBusy}
              className="rounded-lg border border-violet-200 px-4 py-2.5 text-sm text-violet-700 transition-colors hover:bg-violet-50 disabled:opacity-60"
            >
              {reviewBusy ? "重新检查中..." : "重新检查当前计划"}
            </button>
          </div>
          <button
            type="button"
            onClick={onReset}
            className="rounded-lg border border-slate-200 px-4 py-2.5 text-sm text-slate-400 transition-colors hover:text-slate-600"
          >
            重来
          </button>
        </div>
      </div>
    </div>
  );
}
