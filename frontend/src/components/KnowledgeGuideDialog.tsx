import type { KnowledgeStatus } from "../shared/api/index.ts";

interface KnowledgeGuideDialogProps {
  open: boolean;
  status: KnowledgeStatus | null;
  onClose: () => void;
}

export function KnowledgeGuideDialog({ open, status, onClose }: KnowledgeGuideDialogProps) {
  if (!open) {
    return null;
  }

  const gameSourceLabel =
    status?.game?.source_mode === "runtime_decompiled"
      ? "运行时反编译目录"
      : status?.game?.source_mode === "repo_reference"
        ? "仓库静态参考"
        : "缺失";
  const baselibSourceLabel =
    status?.baselib?.source_mode === "runtime_decompiled"
      ? "运行时反编译目录"
      : status?.baselib?.source_mode === "repo_fallback"
        ? "仓库内 BaseLib fallback"
        : "缺失";

  return (
    <div className="fixed inset-0 z-[60] bg-black/50 flex items-center justify-center px-4" onClick={onClose}>
      <div
        className="w-full max-w-2xl rounded-2xl border border-slate-200 bg-white p-6 shadow-2xl space-y-5"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="space-y-1">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-slate-400">Knowledge Guide</p>
          <h3 className="text-xl font-semibold text-slate-900">知识库说明</h3>
          <p className="text-sm text-slate-500">这里说明知识库的版本来源，以及本工程里的缓存与反编译位置。</p>
        </div>

        <div className="grid gap-4 md:grid-cols-2">
          <section className="rounded-xl border border-slate-200 bg-slate-50 p-4 space-y-2">
            <h4 className="text-sm font-semibold text-slate-800">游戏知识库来源</h4>
            <p className="text-xs text-slate-500">自动检测到的 `sts2_path` 与 Steam `app manifest / 安装版本文本`。</p>
            <p className="text-xs text-slate-600 break-all">当前路径：{status?.game?.configured_path || "未检测到"}</p>
            <p className="text-xs text-slate-600">当前版本：{status?.game?.current_version || status?.game?.version || "未知"}</p>
            <p className="text-xs text-slate-600">当前实际使用来源：{gameSourceLabel}</p>
            <p className="text-xs text-slate-600 break-all">当前知识路径：{status?.game?.decompiled_src_path || "未检测到"}</p>
          </section>

          <section className="rounded-xl border border-slate-200 bg-slate-50 p-4 space-y-2">
            <h4 className="text-sm font-semibold text-slate-800">Baselib 知识库来源</h4>
            <p className="text-xs text-slate-500">GitHub latest release：`BaseLib-StS2 releases`</p>
            <a
              href={status?.baselib?.release_url || "https://github.com/Alchyr/BaseLib-StS2/releases"}
              target="_blank"
              rel="noreferrer"
              className="text-xs text-amber-700 hover:text-amber-800 underline break-all"
            >
              https://github.com/Alchyr/BaseLib-StS2/releases
            </a>
            <p className="text-xs text-slate-600">当前 release：{status?.baselib?.latest_release_tag || status?.baselib?.release_tag || "未知"}</p>
            <p className="text-xs text-slate-600">当前实际使用来源：{baselibSourceLabel}</p>
            <p className="text-xs text-slate-600 break-all">当前知识路径：{status?.baselib?.decompiled_src_path || "未检测到"}</p>
          </section>
        </div>

        <section className="rounded-xl border border-slate-200 bg-white p-4 space-y-2">
          <h4 className="text-sm font-semibold text-slate-800">本工程内位置</h4>
          <ul className="space-y-1 text-xs text-slate-600 font-mono">
            <li>runtime/knowledge/knowledge-manifest.json</li>
            <li>runtime/knowledge/game_decompiled/</li>
            <li>runtime/knowledge/baselib_decompiled/</li>
            <li>runtime/knowledge/cache/</li>
          </ul>
        </section>

        <div className="flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-200 px-4 py-2 text-sm text-slate-600 hover:border-amber-300 hover:text-amber-700 transition-colors"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
