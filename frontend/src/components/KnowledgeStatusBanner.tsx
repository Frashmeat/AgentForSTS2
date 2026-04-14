import type { KnowledgeStatus } from "../shared/api/knowledge.ts";

interface KnowledgeStatusBannerProps {
  status: KnowledgeStatus | null;
  onOpenGuide: () => void;
  onOpenSettings: () => void;
}

function resolveStatusLabel(status: KnowledgeStatus["status"] | undefined) {
  switch (status) {
    case "fresh":
      return "正常";
    case "stale":
      return "状态待确认";
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

function resolveSourceLabel(sourceMode: string | undefined) {
  return sourceMode === "runtime_decompiled" ? "运行时知识目录" : "缺失";
}

export function KnowledgeStatusBanner({
  status,
  onOpenGuide,
  onOpenSettings,
}: KnowledgeStatusBannerProps) {
  if (status === null) {
    return null;
  }

  const gameKnowledgePath = status.game.knowledge_path || "未检测到";
  const baselibKnowledgePath = status.baselib.knowledge_path || "未检测到";

  return (
    <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-3 text-xs text-slate-700 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="font-semibold text-slate-800">当前知识库信息</p>
        <span className="rounded-full border border-slate-200 bg-white px-2 py-0.5 text-[11px] text-slate-600">
          状态：{resolveStatusLabel(status.status)}
        </span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <div className="space-y-1">
          <p className="font-medium text-slate-700">游戏知识库</p>
          <p>来源：{resolveSourceLabel(status.game.source_mode)}</p>
          <p>版本：{status.game.current_version || status.game.version || "未知"}</p>
          <p className="break-all">路径：{gameKnowledgePath}</p>
        </div>
        <div className="space-y-1">
          <p className="font-medium text-slate-700">Baselib 知识库</p>
          <p>来源：{resolveSourceLabel(status.baselib.source_mode)}</p>
          <p>Release：{status.baselib.latest_release_tag || status.baselib.release_tag || "未知"}</p>
          <p className="break-all">路径：{baselibKnowledgePath}</p>
        </div>
      </div>
      {status.warnings.length > 0 && (
        <div className="rounded-md border border-slate-200 bg-white px-2.5 py-2 text-slate-600">
          <p className="font-medium text-slate-700">状态说明</p>
          <div className="mt-1 space-y-1">
            {status.warnings.map((warning) => (
              <p key={warning} className="leading-5">- {warning}</p>
            ))}
          </div>
        </div>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onOpenSettings}
          className="rounded-md border border-slate-200 bg-white px-2.5 py-1 font-medium text-slate-700 hover:border-slate-300 hover:bg-slate-100 transition-colors"
        >
          打开设置
        </button>
        <button
          type="button"
          onClick={onOpenGuide}
          className="rounded-md border border-slate-200 bg-white px-2.5 py-1 font-medium text-slate-600 hover:border-slate-300 hover:text-slate-800 transition-colors"
        >
          查看说明
        </button>
      </div>
    </div>
  );
}
