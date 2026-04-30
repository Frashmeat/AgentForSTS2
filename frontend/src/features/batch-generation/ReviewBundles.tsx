// 执行策略 bundle 决策视图：依赖分组预览 + 每个 bundle 的风险解释 + 决策按钮（接受/拆分/返回）。
// 从 view.tsx 抽出，纯 props-driven。

import type { PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
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

  return (
    <div className="space-y-4">
      <div className="workspace-surface rounded-2xl p-5 space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-violet-500">Step 2 / 2</p>
            <h2 className="font-bold text-slate-800">执行策略决策</h2>
            <p className="text-xs text-slate-500 mt-0.5">
              先决定哪些 item 必须一起执行、哪些应拆开执行，处理完待决策 bundle 后再进入真正执行。
            </p>
          </div>
          <span className="text-xs text-violet-700 bg-violet-50 border border-violet-200 rounded-full px-2 py-0.5 font-medium">
            {progress.clear + progress.accepted}/{executionBundles.length || 0} 个 bundle 已具备执行条件
          </span>
        </div>

        <ReviewStrictnessSelector value={reviewStrictness} disabled={reviewBusy} onChange={onStrictnessChange} />

        <ReviewFeedbackBanner feedback={reviewFeedback} />
        <ReviewNotice message={reviewError} />

        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">待你确认</p>
            <p className="mt-1 text-2xl font-semibold text-slate-800">{progress.unresolved}</p>
            <p className="mt-1 text-xs text-slate-500">系统已给出风险判断，但还没有收到你的明确决策。</p>
          </div>
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">已接受当前分组</p>
            <p className="mt-1 text-2xl font-semibold text-green-700">{progress.accepted}</p>
            <p className="mt-1 text-xs text-slate-500">这些 bundle 即使带风险，也已经被你显式接受。</p>
          </div>
          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-medium text-slate-500">仍阻塞执行</p>
            <p className="mt-1 text-2xl font-semibold text-amber-700">{progress.blocking}</p>
            <p className="mt-1 text-xs text-slate-500">包含待决策、待拆分重算或待回到 Item 复核的 bundle。</p>
          </div>
        </div>

        <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
          <p className="text-sm font-medium text-slate-700">依赖分组预览</p>
          <p className="mt-1 text-xs text-slate-500">
            这里展示的是依赖关系视角；下方的执行 Bundle 是系统综合耦合度和风险后给出的执行视角。
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
            <div className="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-6 text-center text-sm text-slate-400">
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
              <div key={`bundle-${index}`} className="rounded-xl border border-slate-200 bg-white px-4 py-4 space-y-3">
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
                        "text-xs rounded-full border px-2 py-0.5 font-medium",
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
                <div className="rounded-lg bg-slate-50 px-3 py-3 space-y-2">
                  <p className="text-xs font-medium text-slate-500">为什么当前会阻塞</p>
                  <p className="text-sm text-slate-700">{getBundleBlockingReason(bundle)}</p>
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
                        {detail.impact ? <p className="mt-1 text-xs text-slate-500">影响：{detail.impact}</p> : null}
                        <p className="mt-2 text-xs text-slate-600">建议：{detail.recommendation}</p>
                      </div>
                    ))}
                  </div>
                )}
                {recommendedActions.length > 0 && (
                  <div className="rounded-lg border border-slate-200 bg-white px-3 py-3 space-y-3">
                    <div>
                      <p className="text-xs font-medium text-slate-500">建议动作</p>
                      <p className="mt-1 text-xs text-slate-500">处理完当前 bundle 的决策后，底部主按钮才会放行。</p>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {recommendedActions.some((action) => action.action === "accept_bundle") && (
                        <button
                          type="button"
                          onClick={() => onBundleDecisionChange(bundleKey, "accepted")}
                          className="rounded-lg bg-emerald-600 px-3 py-2 text-xs font-medium text-white hover:bg-emerald-700 transition-colors"
                        >
                          接受当前分组
                        </button>
                      )}
                      {recommendedActions.some((action) => action.action === "split_bundle") && (
                        <button
                          type="button"
                          onClick={() => onBundleSplitRequest(bundleKey)}
                          disabled={reviewBusy}
                          className="rounded-lg bg-sky-600 px-3 py-2 text-xs font-medium text-white hover:bg-sky-700 transition-colors disabled:opacity-60"
                        >
                          {reviewBusy ? "重算中..." : "要求拆分"}
                        </button>
                      )}
                      {recommendedActions.some((action) => action.action === "revise_items") && (
                        <button
                          type="button"
                          onClick={() => onBundleReturnToItems(bundleKey, bundle.item_ids)}
                          className="rounded-lg border border-slate-200 px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 transition-colors"
                        >
                          返回补充说明
                        </button>
                      )}
                    </div>
                    <div className="grid gap-2 md:grid-cols-3">
                      {recommendedActions.map((action) => (
                        <div key={`${bundleKey}-${action.action}`} className="rounded-lg bg-slate-50 px-3 py-2">
                          <p className="text-xs font-medium text-slate-700">{action.label}</p>
                          <p className="mt-1 text-xs text-slate-500">{action.description}</p>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <div className="flex flex-wrap gap-2 mt-4">
          <button
            type="button"
            onClick={onBack}
            className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-600 hover:text-slate-800 text-sm transition-colors"
          >
            返回 Item 复核
          </button>
          <button
            type="button"
            onClick={onRefreshReview}
            disabled={reviewBusy}
            className="py-2.5 px-4 rounded-lg border border-violet-200 text-violet-700 text-sm hover:bg-violet-50 transition-colors disabled:opacity-60"
          >
            {reviewBusy ? "重新检查中..." : "重新检查当前计划"}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={reviewBusy}
            className="flex-1 py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 transition-colors disabled:opacity-60"
          >
            {canProceedFromBundleReview(review, bundleDecisions)
              ? "确认执行策略，开始执行"
              : `先处理剩余 ${progress.blocking} 个阻塞 bundle`}
          </button>
          <button
            type="button"
            onClick={onReset}
            className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-400 hover:text-slate-600 text-sm transition-colors"
          >
            重来
          </button>
        </div>
      </div>
    </div>
  );
}
