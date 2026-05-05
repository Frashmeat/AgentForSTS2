// 计划复核第一步：逐项确认 item 描述、关系、影响、图片来源等。
// 从 view.tsx 抽出，本地状态：expandedId / uploadPreviews。

import { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronUp, Copy, Plus, Trash2, Upload, Wand2 } from "lucide-react";

import type { ModPlan, PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
import { ReviewFeedbackBanner, ReviewNotice, ReviewStatusBadge, ReviewStrictnessSelector } from "./ReviewBadges.tsx";
import type { ReviewStrictness } from "./state.ts";
import { TYPE_LABELS, type ReviewFeedback } from "./view-constants.ts";
import { canProceedFromEditedItemReview } from "./view-helpers.ts";

const RELATIONSHIP_OPTIONS = [
  {
    value: "unknown",
    label: "不确定关系",
    helper: "暂时拿不准是否要合并或保持顺序，让 AI 重新审查。",
  },
  {
    value: "independent",
    label: "独立执行",
    helper: "这个 item 可以单独完成，不需要等待或合并其他 item。",
  },
  {
    value: "ordered_dependency",
    label: "有先后顺序",
    helper: "只要求选中的 item 先完成，但不默认合并执行。",
  },
  {
    value: "same_feature",
    label: "同一功能组",
    helper: "这些 item 是同一玩法或功能的一部分，通常适合一起执行。",
  },
  {
    value: "shared_mechanism",
    label: "共享机制或资源",
    helper: "这些 item 共享代码机制、注册入口、状态、图片或资源上下文。",
  },
] as const;

type RelationshipType = (typeof RELATIONSHIP_OPTIONS)[number]["value"];

const ITEM_TYPE_OPTIONS = [
  "card",
  "card_fullscreen",
  "relic",
  "power",
  "character",
  "custom_code",
] as const;

const DEFAULT_RELATIONSHIP_TYPE: RelationshipType = "unknown";

function createItemId(existingIds: Set<string>) {
  let index = existingIds.size + 1;
  let candidate = `custom_item_${index}`;
  while (existingIds.has(candidate)) {
    index += 1;
    candidate = `custom_item_${index}`;
  }
  return candidate;
}

function createNewItem(existingIds: Set<string>): PlanItem {
  const id = createItemId(existingIds);
  return {
    id,
    type: "custom_code",
    name: "NewCustomItem",
    name_zhs: "",
    description: "",
    goal: "",
    detailed_description: "",
    implementation_notes: "",
    needs_image: false,
    image_description: "",
    depends_on_item_ids: [],
    scope_boundary: "",
    relationship_reason: "",
    acceptance_notes: "",
    affected_targets: [],
    relationship_type: DEFAULT_RELATIONSHIP_TYPE,
    clarification_status: "",
    clarification_questions: [],
  };
}

function normalizeRelationshipType(value: string): RelationshipType {
  return RELATIONSHIP_OPTIONS.some((option) => option.value === value)
    ? (value as RelationshipType)
    : DEFAULT_RELATIONSHIP_TYPE;
}

function relationLabel(value: string) {
  return RELATIONSHIP_OPTIONS.find((option) => option.value === value)?.label ?? "不确定关系";
}

function relationSummary(item: PlanItem) {
  const count = item.depends_on_item_ids.length;
  switch (normalizeRelationshipType(item.relationship_type)) {
    case "independent":
      return "独立执行";
    case "ordered_dependency":
      return count > 0 ? `先做 ${count} 项` : "待选择前置项";
    case "same_feature":
      return count > 0 ? `同组 ${count} 项` : "同一功能组";
    case "shared_mechanism":
      return count > 0 ? `共享 ${count} 项` : "共享机制或资源";
    case "unknown":
    default:
      return "待 AI 判断";
  }
}

function sanitizeItemsAfterDelete(items: PlanItem[], deletedId: string) {
  return items.map((item) => ({
    ...item,
    depends_on_item_ids: item.depends_on_item_ids.filter((id) => id !== deletedId),
  }));
}

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
  const existingIds = useMemo(() => new Set(editedItems.map((item) => item.id)), [editedItems]);

  useEffect(() => {
    if (focusItemId) {
      setExpandedId(focusItemId);
    }
  }, [focusItemId]);

  function updateItem(id: string, patch: Partial<PlanItem>) {
    setEditedItems(editedItems.map((it) => (it.id === id ? { ...it, ...patch } : it)));
  }

  function updateItemId(previousId: string, nextId: string) {
    const normalizedId = nextId.trim();
    setEditedItems(
      editedItems.map((item) => {
        if (item.id === previousId) {
          return { ...item, id: normalizedId };
        }
        return {
          ...item,
          depends_on_item_ids: item.depends_on_item_ids.map((id) => (id === previousId ? normalizedId : id)),
        };
      }),
    );
    setExpandedId(normalizedId);
  }

  function updateStringList(id: string, field: "affected_targets", value: string) {
    updateItem(id, {
      [field]: value
        .split(/[\n,]/)
        .map((entry) => entry.trim())
        .filter(Boolean),
    } as Partial<PlanItem>);
  }

  function addItem() {
    const item = createNewItem(existingIds);
    setEditedItems([...editedItems, item]);
    setExpandedId(item.id);
  }

  function copyItem(source: PlanItem) {
    const id = createItemId(existingIds);
    const item = {
      ...source,
      id,
      name: `${source.name || "CopiedItem"}Copy`,
      name_zhs: source.name_zhs ? `${source.name_zhs}副本` : source.name_zhs,
      depends_on_item_ids: source.depends_on_item_ids.filter((depId) => depId !== source.id),
      clarification_status: "",
      clarification_questions: [],
      provided_image_b64: undefined,
    };
    setEditedItems([...editedItems, item]);
    setExpandedId(id);
  }

  function deleteItem(id: string) {
    const nextItems = sanitizeItemsAfterDelete(
      editedItems.filter((item) => item.id !== id),
      id,
    );
    setEditedItems(nextItems);
    setExpandedId((current) => (current === id ? nextItems[0]?.id ?? null : current));
    setUploadPreviews((previews) => {
      const next = { ...previews };
      delete next[id];
      return next;
    });
  }

  function updateRelationshipType(item: PlanItem, relationshipType: RelationshipType) {
    updateItem(item.id, {
      relationship_type: relationshipType,
      depends_on_item_ids:
        relationshipType === "independent" || relationshipType === "unknown" ? [] : item.depends_on_item_ids,
    });
  }

  function toggleRelatedItem(item: PlanItem, relatedId: string) {
    const selected = new Set(item.depends_on_item_ids);
    if (selected.has(relatedId)) {
      selected.delete(relatedId);
    } else {
      selected.add(relatedId);
    }
    updateItem(item.id, { depends_on_item_ids: Array.from(selected) });
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

          <div className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 md:flex-row md:items-center md:justify-between">
            <div>
              <p className="text-sm font-medium text-slate-700">当前阶段：逐项确认 Item</p>
              <p className="mt-1 text-xs text-slate-500">
                你可以手动新增、复制、删除 item；修改目标、范围和关系后，再让 AI 重新审查是否能进入执行策略。
              </p>
              {!canProceed && (
                <p className="mt-2 text-xs font-medium text-amber-700">
                  仍有 item 需要补充说明。点击右侧按钮后，AI 会基于当前清单重新判断。
                </p>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={addItem}
                className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-slate-200 bg-white px-3 py-2.5 text-sm font-bold text-slate-700 transition-colors hover:bg-slate-100"
              >
                <Plus size={14} />
                新增 Item
              </button>
              <button
                type="button"
                onClick={onRefreshReview}
                disabled={reviewBusy}
                className="inline-flex items-center justify-center gap-1.5 rounded-lg bg-violet-700 px-4 py-2.5 text-sm font-bold text-white transition-colors hover:bg-violet-800 disabled:opacity-60"
              >
                <Wand2 size={14} />
                {reviewBusy ? "AI 审查中..." : "让 AI 重新审查 Item"}
              </button>
            </div>
          </div>
        </div>

        <ReviewFeedbackBanner feedback={reviewFeedback} />
        <ReviewNotice message={reviewError} />

        <div className="space-y-2">
          {editedItems.map((item, index) => {
            const validation = validationById.get(item.id);
            const missingFields = validation?.missing_fields ?? [];
            const issues = validation?.issues ?? [];
            const questions = validation?.clarification_questions ?? [];
            const normalizedRelationshipType = normalizeRelationshipType(item.relationship_type);
            const relatedOptions = editedItems.filter((candidate) => candidate.id !== item.id);
            const showRelatedPicker =
              normalizedRelationshipType === "ordered_dependency" ||
              normalizedRelationshipType === "same_feature" ||
              normalizedRelationshipType === "shared_mechanism";

            return (
              <div key={index} className="rounded-lg border border-slate-200 bg-slate-50 overflow-hidden">
                <div className="flex items-center gap-2 px-3 py-2.5 hover:bg-slate-100 transition-colors">
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-3 text-left"
                    onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
                  >
                    <span className="text-xs font-medium text-slate-500 bg-slate-200 rounded px-1.5 py-0.5 shrink-0">
                      {TYPE_LABELS[item.type] ?? item.type}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-slate-700">{item.name}</span>
                    <span className="hidden text-xs text-slate-400 sm:inline">{relationSummary(item)}</span>
                    {validation && <ReviewStatusBadge status={validation.status} kind="item" />}
                    {expandedId === item.id ? (
                      <ChevronUp size={13} className="text-slate-400 shrink-0" />
                    ) : (
                      <ChevronDown size={13} className="text-slate-400 shrink-0" />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => copyItem(item)}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 hover:bg-white hover:text-slate-700"
                    aria-label="复制 Item"
                    title="复制 Item"
                  >
                    <Copy size={14} />
                  </button>
                  <button
                    type="button"
                    onClick={() => deleteItem(item.id)}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 hover:bg-red-50 hover:text-red-600"
                    aria-label="删除 Item"
                    title="删除 Item"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>

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

                    <div className="grid gap-3 md:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">Item ID</label>
                        <input
                          value={item.id}
                          onChange={(e) => updateItemId(item.id, e.target.value)}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm font-mono focus:outline-none focus:border-violet-400"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">类型</label>
                        <select
                          value={item.type}
                          onChange={(e) => {
                            const nextType = e.target.value;
                            updateItem(item.id, {
                              type: nextType,
                              needs_image: nextType !== "custom_code",
                            });
                          }}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-2 text-sm focus:outline-none focus:border-violet-400"
                        >
                          {ITEM_TYPE_OPTIONS.map((type) => (
                            <option key={type} value={type}>
                              {TYPE_LABELS[type] ?? type}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>

                    <div className="grid gap-3 md:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">名称（英文）</label>
                        <input
                          value={item.name}
                          onChange={(e) => updateItem(item.id, { name: e.target.value })}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">中文名</label>
                        <input
                          value={item.name_zhs}
                          onChange={(e) => updateItem(item.id, { name_zhs: e.target.value })}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                        />
                      </div>
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
                        <label className="text-xs text-slate-400">关系说明</label>
                        <textarea
                          value={item.relationship_reason}
                          onChange={(e) => updateItem(item.id, { relationship_reason: e.target.value })}
                          rows={3}
                          placeholder="说明为什么要先后执行、同组执行，或共享机制/资源。"
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
                      <div className="space-y-2">
                        <label className="text-xs text-slate-400">Item 关系</label>
                        <select
                          value={normalizedRelationshipType}
                          onChange={(e) => updateRelationshipType(item, e.target.value as RelationshipType)}
                          className="w-full bg-white border border-slate-200 rounded px-2 py-2 text-sm focus:outline-none focus:border-violet-400"
                        >
                          {RELATIONSHIP_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <p className="text-xs text-slate-500">
                          {RELATIONSHIP_OPTIONS.find((option) => option.value === normalizedRelationshipType)?.helper}
                        </p>
                      </div>
                    </div>

                    {showRelatedPicker && (
                      <div className="space-y-2 rounded-lg border border-slate-200 bg-white p-3">
                        <div className="flex items-center justify-between gap-3">
                          <label className="text-xs font-medium text-slate-500">
                            {normalizedRelationshipType === "ordered_dependency"
                              ? "需要先完成的 Item"
                              : `${relationLabel(normalizedRelationshipType)}相关 Item`}
                          </label>
                          <span className="text-xs text-slate-400">{item.depends_on_item_ids.length} 项已选</span>
                        </div>
                        {relatedOptions.length === 0 ? (
                          <p className="text-xs text-slate-400">当前没有其他 item 可选择。</p>
                        ) : (
                          <div className="flex flex-wrap gap-2">
                            {relatedOptions.map((candidate) => {
                              const selected = item.depends_on_item_ids.includes(candidate.id);
                              return (
                                <button
                                  key={candidate.id}
                                  type="button"
                                  onClick={() => toggleRelatedItem(item, candidate.id)}
                                  className={cn(
                                    "rounded-full border px-3 py-1 text-xs transition-colors",
                                    selected
                                      ? "border-violet-300 bg-violet-50 text-violet-700"
                                      : "border-slate-200 bg-slate-50 text-slate-500 hover:bg-slate-100",
                                  )}
                                >
                                  {candidate.name || candidate.id}
                                </button>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    )}

                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">影响目标（逗号或换行分隔）</label>
                      <textarea
                        value={item.affected_targets.join("\n")}
                        onChange={(e) => updateStringList(item.id, "affected_targets", e.target.value)}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>

                    {item.needs_image && (
                      <div className="space-y-2">
                        <div className="flex items-center gap-1.5">
                          <span className="text-xs text-slate-400">图片来源：</span>
                          <button
                            type="button"
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
                            type="button"
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
                        {item.provided_image_b64 && uploadPreviews[item.id] && (
                          <div className="relative w-24 h-24 rounded-lg overflow-hidden border border-violet-300">
                            <img src={uploadPreviews[item.id]} alt="preview" className="w-full h-full object-cover" />
                            <button
                              type="button"
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
                              x
                            </button>
                          </div>
                        )}
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
            onClick={onConfirm}
            disabled={reviewBusy}
            className="flex-1 py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 transition-colors disabled:opacity-60"
          >
            {canProceed ? "进入执行策略决策" : "先让 AI 审查 Item"}
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
