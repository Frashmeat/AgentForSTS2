import type { KnowledgeStatus } from "../shared/api/knowledge.ts";

interface KnowledgeStatusBannerProps {
  status: KnowledgeStatus | null;
  impactText: string;
  onOpenGuide: () => void;
  onOpenSettings: () => void;
}

export function KnowledgeStatusBanner({
  status,
  impactText,
  onOpenGuide,
  onOpenSettings,
}: KnowledgeStatusBannerProps) {
  if (status?.status !== "stale" && status?.status !== "missing") {
    return null;
  }

  const message =
    status.status === "missing"
      ? `当前未检测到可用知识库，本次将以内置参考继续执行，${impactText}。`
      : `当前知识库可能不是最新版本，${impactText}，建议先检查更新。`;

  return (
    <div className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs text-amber-800 space-y-2">
      <p>{message}</p>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onOpenSettings}
          className="rounded-md border border-amber-300 px-2.5 py-1 font-medium text-amber-700 hover:bg-amber-100 transition-colors"
        >
          打开设置
        </button>
        <button
          type="button"
          onClick={onOpenGuide}
          className="rounded-md border border-slate-200 px-2.5 py-1 font-medium text-slate-600 hover:border-amber-300 hover:text-amber-700 transition-colors"
        >
          查看说明
        </button>
      </div>
    </div>
  );
}
