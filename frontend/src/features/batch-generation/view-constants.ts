// 从 view.tsx 抽出的纯常量与类型；无 JSX，无 React 依赖。
// STATUS_ICONS 含 JSX 仍留在 view.tsx 里。

import type {
  ExecutionBundlePreview,
  ExecutionBundleRecommendedAction,
  ExecutionBundleRiskDetail,
  PlanItemValidation,
} from "../../shared/types/workflow.ts";
import type { BundleDecisionStatus, ReviewStrictness } from "./state.ts";

export type ReviewFeedbackTone = "info" | "success" | "warning" | "error";

export interface ReviewFeedback {
  tone: ReviewFeedbackTone;
  message: string;
}

export const TYPE_LABELS: Record<string, string> = {
  card: "卡牌",
  card_fullscreen: "全画面卡",
  relic: "遗物",
  power: "Power",
  character: "角色",
  custom_code: "代码",
};

export const STATUS_LABELS: Record<string, string> = {
  pending: "等待中",
  img_generating: "生成图像",
  awaiting_selection: "等待选图",
  approval_pending: "等待审批",
  code_generating: "生成代码",
  cancelled: "已取消",
  done: "完成",
  error: "失败",
};

export const PLAN_STORAGE_KEY = "ats_last_plan";
export const PLAN_ITEMS_STORAGE_KEY = "ats_last_plan_items";
export const PLAN_REVIEW_STORAGE_KEY = "ats_last_plan_review";
export const PLAN_REVIEW_STRICTNESS_STORAGE_KEY = "ats_last_plan_review_strictness";
export const PLAN_BUNDLE_DECISIONS_STORAGE_KEY = "ats_last_plan_bundle_decisions";

export const REVIEW_STATUS_LABELS: Record<PlanItemValidation["status"], string> = {
  clear: "可继续",
  needs_user_input: "待补充",
  invalid: "存在错误",
};

export const BUNDLE_STATUS_LABELS: Record<ExecutionBundlePreview["status"], string> = {
  clear: "可执行",
  needs_confirmation: "需确认",
  split_recommended: "建议拆分",
};

export const STRICTNESS_OPTIONS: Array<{
  value: ReviewStrictness;
  label: string;
  description: string;
}> = [
  { value: "efficient", label: "高效率", description: "减少拦截，尽快进入执行" },
  { value: "balanced", label: "平衡", description: "兼顾确认成本和执行安全" },
  { value: "strict", label: "严格", description: "更细地检查描述和分组风险" },
];

export const BUNDLE_DECISION_LABELS: Record<BundleDecisionStatus, string> = {
  unresolved: "待决策",
  accepted: "已接受当前分组",
  split_requested: "已要求拆分",
  needs_item_revision: "待回到 Item 补充说明",
};

export const BUNDLE_DECISION_TONES: Record<BundleDecisionStatus, string> = {
  unresolved: "bg-amber-50 text-amber-700 border-amber-200",
  accepted: "bg-green-50 text-green-700 border-green-200",
  split_requested: "bg-sky-50 text-sky-700 border-sky-200",
  needs_item_revision: "bg-rose-50 text-rose-700 border-rose-200",
};

export const FALLBACK_BUNDLE_RISK_DETAILS: Record<string, ExecutionBundleRiskDetail> = {
  unclear_coupling: {
    code: "unclear_coupling",
    title: "耦合关系不明确",
    summary: "系统无法确认这些 item 是否必须绑在一起执行。",
    recommendation: "如果你确认它们必须一起落地，就接受当前分组；如果只是可能相关，优先返回补充依赖说明或要求拆分。",
    impact: "错误合并后会放大一次执行的影响范围。",
  },
  bundle_size_threshold: {
    code: "bundle_size_threshold",
    title: "Bundle 规模偏大",
    summary: "当前分组包含的 item 偏多，失败后的回滚和定位成本会明显上升。",
    recommendation: "优先拆分为更小的执行单元；只有在这些 item 明显属于同一功能包时再接受当前分组。",
    impact: "一次执行覆盖面过大，排错节奏会变慢。",
  },
  mixed_item_types: {
    code: "mixed_item_types",
    title: "包含多种 item 类型",
    summary: "同一 bundle 中混入了不同类型的产物，执行节奏和关注点不一致。",
    recommendation: "若只是共享目标但不是强耦合，建议拆开；若它们围绕同一核心功能联合交付，再接受当前分组。",
    impact: "混合类型越多，执行和验收口径越容易漂移。",
  },
  affected_targets_spread: {
    code: "affected_targets_spread",
    title: "影响范围过散",
    summary: "这组 item 触及的目标点较多，说明分组边界可能过宽。",
    recommendation: "优先回到 Item 补充范围说明，或要求先拆分后再重算。",
    impact: "范围过散会增加一次执行失败波及多个模块的概率。",
  },
};

export const FALLBACK_BUNDLE_ACTIONS: Record<ExecutionBundlePreview["status"], ExecutionBundleRecommendedAction[]> = {
  clear: [],
  needs_confirmation: [
    {
      action: "accept_bundle",
      label: "接受当前分组",
      description: "你确认这些 item 应该作为一个 bundle 联合执行。",
      emphasis: "primary",
    },
    {
      action: "revise_items",
      label: "返回补充说明",
      description: "回到 Item 层补充依赖原因、范围边界或验收说明后重算。",
      emphasis: "secondary",
    },
  ],
  split_recommended: [
    {
      action: "split_bundle",
      label: "要求拆分",
      description: "按更保守的口径重算，把该 bundle 拆成更小执行单元。",
      emphasis: "warning",
    },
    {
      action: "accept_bundle",
      label: "仍接受当前分组",
      description: "你确认这些 item 必须一起执行，即使系统建议拆分。",
      emphasis: "primary",
    },
    {
      action: "revise_items",
      label: "返回补充说明",
      description: "回到 Item 层补充依赖说明后再重算，降低误判概率。",
      emphasis: "secondary",
    },
  ],
};
