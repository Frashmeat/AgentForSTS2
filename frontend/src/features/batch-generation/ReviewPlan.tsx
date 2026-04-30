// 计划复核第一步：逐项确认 item 描述、依赖、影响、图片来源等。
// 从 view.tsx 抽出，本地状态：expandedId / uploadPreviews。

import { useEffect, useState } from "react";
import { ChevronDown, ChevronUp, Upload, Wand2 } from "lucide-react";

import type { ModPlan, PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
import { ReviewFeedbackBanner, ReviewNotice, ReviewStatusBadge, ReviewStrictnessSelector } from "./ReviewBadges.tsx";
import type { ReviewStrictness } from "./state.ts";
import { TYPE_LABELS, type ReviewFeedback } from "./view-constants.ts";
import { canProceedFromEditedItemReview } from "./view-helpers.ts";

export function ReviewPlan({
  plan,
  review,
  reviewStrictness,
  reviewBusy,
  reviewError,
  reviewFeedback,
  focusItemId,
  editedItems,
  setEditedItems,
  onRefreshReview,
  onStrictnessChange,
  onConfirm,
  onReset,
}: {
  plan: ModPlan;
  review: PlanReviewPayload | null;
  reviewStrictness: ReviewStrictness;
  reviewBusy: boolean;
  reviewError: string | null;
  reviewFeedback: ReviewFeedback | null;
  focusItemId: string | null;
  editedItems: PlanItem[];
  setEditedItems: (items: PlanItem[]) => void;
  onRefreshReview: () => void;
  onStrictnessChange: (value: ReviewStrictness) => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [uploadPreviews, setUploadPreviews] = useState<Record<string, string>>({});
  const validationById = new Map((review?.validation.items ?? []).map((item) => [item.item_id, item]));
  const clearCount = review?.validation.items.filter((item) => item.status === "clear").length ?? 0;
  const canProceed = canProceedFromEditedItemReview(review, editedItems);

  useEffect(() => {
    if (focusItemId) {
      setExpandedId(focusItemId);
    }
  }, [focusItemId]);

  function updateItem(id: string, patch: Partial<PlanItem>) {
    setEditedItems(editedItems.map((it) => (it.id === id ? { ...it, ...patch } : it)));
  }

  function updateStringList(id: string, field: "depends_on" | "affected_targets", value: string) {
    updateItem(id, {
      [field]: value
        .split(/[\n,]/)
        .map((entry) => entry.trim())
        .filter(Boolean),
    } as Partial<PlanItem>);
  }

  function handleImageFile(id: string, file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const dataUrl = e.target?.result as string;
      const b64 = dataUrl.split(",")[1];
      setUploadPreviews((p) => ({ ...p, [id]: dataUrl }));
      updateItem(id, { provided_image_b64: b64 });
    };
    reader.readAsDataURL(file);
  }

  return (
    <div className="space-y-4">
      <div className="workspace-surface rounded-2xl p-5">
        <div className="flex flex-col gap-4 mb-4">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-xs uppercase tracking-[0.2em] text-violet-500">Step 1 / 2</p>
              <h2 className="font-bold text-slate-800">{plan.mod_name}</h2>
              <p className="text-xs text-slate-500 mt-0.5">{plan.summary}</p>
            </div>
            <span className="text-xs text-violet-700 bg-violet-50 border border-violet-200 rounded-full px-2 py-0.5 font-medium">
              {clearCount}/{editedItems.length} 项可进入下一步
            </span>
          </div>

          <ReviewStrictnessSelector value={reviewStrictness} disabled={reviewBusy} onChange={onStrictnessChange} />

          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-sm font-medium text-slate-700">当前阶段：逐项确认计划描述</p>
            <p className="mt-1 text-xs text-slate-500">
              先把每个 item 的目标、范围、依赖原因和验收说明确认清楚，再进入执行策略分组确认。
            </p>
            {!canProceed && (
              <p className="mt-2 text-xs font-medium text-amber-700">
                仍有 item 需要补充说明。修改字段后，点击下方按钮会重新检查当前计划。
              </p>
            )}
          </div>
        </div>

        <ReviewFeedbackBanner feedback={reviewFeedback} />
        <ReviewNotice message={reviewError} />

        <div className="space-y-2">
          {editedItems.map((item) => {
            const validation = validationById.get(item.id);
            const missingFields = validation?.missing_fields ?? [];
            const issues = validation?.issues ?? [];
            const questions = validation?.clarification_questions ?? [];

            return (
              <div key={item.id} className="rounded-lg border border-slate-200 bg-slate-50 overflow-hidden">
                <button
                  type="button"
                  className="w-full flex items-center gap-3 px-3 py-2.5 text-left hover:bg-slate-100 transition-colors"
                  onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
                >
                  <span className="text-xs font-medium text-slate-400 bg-slate-200 rounded px-1.5 py-0.5 shrink-0">
                    {TYPE_LABELS[item.type] ?? item.type}
                  </span>
                  <span className="text-sm font-medium text-slate-700 flex-1">{item.name}</span>
                  {validation && <ReviewStatusBadge status={validation.status} kind="item" />}
                  {item.depends_on.length > 0 && (
                    <span className="text-xs text-slate-400">依赖 {item.depends_on.length}</span>
                  )}
                  {expandedId === item.id ? (
                    <ChevronUp size={13} className="text-slate-400 shrink-0" />
                  ) : (
                    <ChevronDown size={13} className="text-slate-400 shrink-0" />
                  )}
                </button>

                {expandedId === item.id && (
                  <div className="px-3 pb-3 space-y-3 border-t border-slate-200 pt-2.5">
                    {validation && (
                      <div className="rounded-xl border border-slate-200 bg-white px-3 py-3 space-y-2">
                        <div className="flex items-center justify-between gap-3">
                          <p className="text-sm font-semibold text-slate-700">当前评审结果</p>
                          <ReviewStatusBadge status={validation.status} kind="item" />
                        </div>
                        {issues.length > 0 && (
                          <div className="flex flex-wrap gap-2">
                            {issues.map((issue) => (
                              <span
                                key={`${issue.code}-${issue.field ?? "base"}`}
                                className="rounded-full bg-red-50 px-2 py-0.5 text-xs text-red-700"
                              >
                                {issue.message}
                              </span>
                            ))}
                          </div>
                        )}
                        {missingFields.length > 0 && (
                          <div className="flex flex-wrap gap-2">
                            {missingFields.map((field) => (
                              <span key={field} className="rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700">
                                待补：{field}
                              </span>
                            ))}
                          </div>
                        )}
                        {questions.length > 0 && (
                          <div className="space-y-1">
                            {questions.map((question, index) => (
                              <p key={`${item.id}-question-${index}`} className="text-xs text-slate-600">
                                {index + 1}. {question}
                              </p>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">名称（英文）</label>
                      <input
                        value={item.name}
                        onChange={(e) => updateItem(item.id, { name: e.target.value })}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">目标</label>
                      <input
                        value={item.goal}
                        onChange={(e) => updateItem(item.id, { goal: e.target.value })}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">详细描述</label>
                      <textarea
                        value={item.detailed_description}
                        onChange={(e) => updateItem(item.id, { detailed_description: e.target.value })}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">用户描述摘要</label>
                      <textarea
                        value={item.description}
                        onChange={(e) => updateItem(item.id, { description: e.target.value })}
                        rows={2}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="grid gap-3 md:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">范围边界</label>
                        <textarea
                          value={item.scope_boundary}
                          onChange={(e) => updateItem(item.id, { scope_boundary: e.target.value })}
                          rows={3}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">依赖原因</label>
                        <textarea
                          value={item.dependency_reason}
                          onChange={(e) => updateItem(item.id, { dependency_reason: e.target.value })}
                          rows={3}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                        />
                      </div>
                    </div>
                    <div className="grid gap-3 md:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">验收说明</label>
                        <textarea
                          value={item.acceptance_notes}
                          onChange={(e) => updateItem(item.id, { acceptance_notes: e.target.value })}
                          rows={3}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">耦合类型</label>
                        <select
                          value={item.coupling_kind}
                          onChange={(e) => updateItem(item.id, { coupling_kind: e.target.value })}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-2 text-sm focus:outline-none focus:border-violet-400"
                        >
                          <option value="unclear">unclear</option>
                          <option value="order_only">order_only</option>
                          <option value="feature_bundle">feature_bundle</option>
                          <option value="shared_logic">shared_logic</option>
                          <option value="isolated">isolated</option>
                        </select>
                      </div>
                    </div>
                    <div className="grid gap-3 md:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">依赖项（逗号或换行分隔）</label>
                        <textarea
                          value={item.depends_on.join("\n")}
                          onChange={(e) => updateStringList(item.id, "depends_on", e.target.value)}
                          rows={3}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">影响目标（逗号或换行分隔）</label>
                        <textarea
                          value={item.affected_targets.join("\n")}
                          onChange={(e) => updateStringList(item.id, "affected_targets", e.target.value)}
                          rows={3}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                        />
                      </div>
                    </div>
                    {item.needs_image && (
                      <div className="space-y-2">
                        {/* 图片模式切换 */}
                        <div className="flex items-center gap-1.5">
                          <span className="text-xs text-slate-400">图片来源：</span>
                          <button
                            onClick={() => updateItem(item.id, { provided_image_b64: undefined })}
                            className={cn(
                              "flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors",
                              !item.provided_image_b64
                                ? "bg-violet-700 text-white"
                                : "bg-slate-100 text-slate-500 hover:bg-slate-200",
                            )}
                          >
                            <Wand2 size={11} /> AI 生成
                          </button>
                          <button
                            onClick={() => {
                              const input = document.createElement("input");
                              input.type = "file";
                              input.accept = "image/*";
                              input.onchange = () => {
                                if (input.files?.[0]) handleImageFile(item.id, input.files[0]);
                              };
                              input.click();
                            }}
                            className={cn(
                              "flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors",
                              item.provided_image_b64
                                ? "bg-violet-700 text-white"
                                : "bg-slate-100 text-slate-500 hover:bg-slate-200",
                            )}
                          >
                            <Upload size={11} /> 上传图片
                          </button>
                        </div>
                        {/* 上传预览 */}
                        {item.provided_image_b64 && uploadPreviews[item.id] && (
                          <div className="relative w-24 h-24 rounded-lg overflow-hidden border border-violet-300">
                            <img src={uploadPreviews[item.id]} alt="preview" className="w-full h-full object-cover" />
                            <button
                              onClick={() => {
                                updateItem(item.id, { provided_image_b64: undefined });
                                setUploadPreviews((p) => {
                                  const n = { ...p };
                                  delete n[item.id];
                                  return n;
                                });
                              }}
                              className="absolute top-0.5 right-0.5 w-4 h-4 rounded-full bg-black/60 text-white text-xs flex items-center justify-center hover:bg-red-500"
                            >
                              ×
                            </button>
                          </div>
                        )}
                        {/* AI 生成时显示图像描述 */}
                        {!item.provided_image_b64 && (
                          <div className="space-y-1">
                            <label className="text-xs text-slate-400">图像描述（AI 生图用）</label>
                            <textarea
                              value={item.image_description}
                              onChange={(e) => updateItem(item.id, { image_description: e.target.value })}
                              rows={2}
                              className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                            />
                          </div>
                        )}
                      </div>
                    )}
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">技术实现说明（给 Code Agent）</label>
                      <textarea
                        value={item.implementation_notes}
                        onChange={(e) => updateItem(item.id, { implementation_notes: e.target.value })}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-xs font-mono resize-none focus:outline-none focus:border-violet-400"
                      />
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
            {canProceed ? "进入执行策略决策" : "保存说明并重新检查"}
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
