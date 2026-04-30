import type { KnowledgeStatus } from "../shared/api/knowledge.ts";

interface KnowledgeStatusTagProps {
  status: KnowledgeStatus | null;
  onOpenGuide: () => void;
}

function resolveStatusLabel(status: KnowledgeStatus["status"] | undefined) {
  switch (status) {
    case "fresh":
      return "正常";
    case "stale":
      return "待确认";
    case "missing":
      return "缺失";
    case "refreshing":
      return "更新中";
    case "checking":
      return "检查中";
    case "error":
      return "读取失败";
    default:
      return "未读取";
  }
}

function resolveStatusDotTone(status: KnowledgeStatus["status"] | undefined) {
  switch (status) {
    case "fresh":
      return "bg-emerald-500";
    case "stale":
      return "bg-amber-500";
    case "missing":
      return "bg-rose-500";
    case "refreshing":
    case "checking":
      return "bg-sky-500";
    case "error":
      return "bg-rose-600";
    default:
      return "bg-slate-400";
  }
}

export function KnowledgeStatusTag({ status, onOpenGuide }: KnowledgeStatusTagProps) {
  const statusLabel = resolveStatusLabel(status?.status);
  const gameVersion = status?.game.current_version || status?.game.version || "未知";
  const baselibVersion = status?.baselib.latest_release_tag || status?.baselib.release_tag || "未知";

  return (
    <button
      type="button"
      onClick={onOpenGuide}
      className="inline-flex max-w-[280px] items-center gap-2 rounded-full border border-slate-200 bg-white/85 px-3 py-1.5 text-xs text-slate-600 transition-colors hover:border-slate-300 hover:bg-white"
      title="查看知识库说明"
      aria-label="查看知识库说明"
    >
      <span className={`h-2 w-2 rounded-full ${resolveStatusDotTone(status?.status)}`} aria-hidden />
      <span className="font-semibold text-slate-700">知识库 {statusLabel}</span>
      {status && (
        <>
          <span className="text-slate-400">|</span>
          <span className="whitespace-nowrap">G {gameVersion}</span>
          <span className="text-slate-400">|</span>
          <span className="whitespace-nowrap">B {baselibVersion}</span>
        </>
      )}
    </button>
  );
}
