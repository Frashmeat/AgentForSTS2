// 从 view.tsx 抽出的小型 review UI 组件：状态徽章 / 严格度选择器 / 提示横幅。

import { Loader2 } from "lucide-react";

import { cn } from "../../lib/utils";
import type { ExecutionBundlePreview, PlanItemValidation } from "../../shared/types/workflow.ts";
import type { ReviewStrictness } from "./state.ts";
import {
  BUNDLE_STATUS_LABELS,
  REVIEW_STATUS_LABELS,
  STRICTNESS_OPTIONS,
  type ReviewFeedback,
} from "./view-constants.ts";

export function ReviewStatusBadge({
  status,
  kind,
}: {
  status: PlanItemValidation["status"] | ExecutionBundlePreview["status"];
  kind: "item" | "bundle";
}) {
  const label =
    kind === "item"
      ? REVIEW_STATUS_LABELS[status as PlanItemValidation["status"]]
      : BUNDLE_STATUS_LABELS[status as ExecutionBundlePreview["status"]];
  const tone =
    status === "clear"
      ? "bg-green-50 text-green-700 border-green-200"
      : status === "invalid"
        ? "bg-red-50 text-red-700 border-red-200"
        : "bg-amber-50 text-amber-700 border-amber-200";
  return <span className={cn("text-xs rounded-full border px-2 py-0.5 font-medium", tone)}>{label}</span>;
}

export function ReviewStrictnessSelector({
  value,
  disabled,
  onChange,
}: {
  value: ReviewStrictness;
  disabled: boolean;
  onChange: (value: ReviewStrictness) => void;
}) {
  return (
    <div className="space-y-2">
      <div>
        <p className="text-sm font-medium text-slate-700">判断严格度</p>
        <p className="text-xs text-slate-400">控制补充说明的严格程度，以及 bundle 拆分时的谨慎程度。</p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {STRICTNESS_OPTIONS.map((option) => (
          <button
            key={option.value}
            type="button"
            disabled={disabled}
            onClick={() => onChange(option.value)}
            className={cn(
              "rounded-xl border px-3 py-2 text-left transition-colors disabled:opacity-60",
              value === option.value
                ? "border-violet-300 bg-violet-50"
                : "border-slate-200 bg-white hover:border-violet-200 hover:bg-violet-50/60",
            )}
          >
            <p className="text-sm font-semibold text-slate-800">{option.label}</p>
            <p className="mt-1 text-xs text-slate-500">{option.description}</p>
          </button>
        ))}
      </div>
    </div>
  );
}

export function ReviewNotice({ message }: { message: string | null }) {
  if (!message) {
    return null;
  }
  return (
    <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">{message}</div>
  );
}

export function ReviewFeedbackBanner({ feedback }: { feedback: ReviewFeedback | null }) {
  if (!feedback) {
    return null;
  }

  const toneCls =
    feedback.tone === "success"
      ? "border-green-200 bg-green-50 text-green-800"
      : feedback.tone === "warning"
        ? "border-amber-200 bg-amber-50 text-amber-800"
        : feedback.tone === "error"
          ? "border-red-200 bg-red-50 text-red-800"
          : "border-violet-200 bg-violet-50 text-violet-800";

  return (
    <div className={cn("rounded-xl border px-4 py-3 text-sm", toneCls)}>
      <div className="flex items-start gap-2">
        {feedback.tone === "info" ? <Loader2 size={16} className="mt-0.5 shrink-0 animate-spin" /> : null}
        <span>{feedback.message}</span>
      </div>
    </div>
  );
}
