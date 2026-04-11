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
      ? "运行时知识目录"
      : "缺失";
  const baselibSourceLabel =
    status?.baselib?.source_mode === "runtime_decompiled"
      ? "运行时知识目录"
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
          <p className="text-sm text-slate-500">这里说明知识库版本来源，以及运行时唯一生效的知识目录位置。</p>
        </div>

        <section className="rounded-xl border border-amber-200 bg-amber-50 p-4 space-y-2">
          <h4 className="text-sm font-semibold text-amber-900">运行时唯一真源</h4>
          <p className="text-xs text-amber-800">
            当前应用只读取 <code>runtime/knowledge/</code> 下的知识文件。发行包会自带这份目录，用户可以直接查看和编辑，后续知识读取会按修改后的内容生效。
          </p>
          <p className="text-xs text-amber-700">
            仓库内的静态文件只用于开发期和打包期生成初始化种子，不再作为运行时并列知识来源。
          </p>
        </section>

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
            <li>runtime/knowledge/game/</li>
            <li>runtime/knowledge/baselib/</li>
            <li>runtime/knowledge/resources/sts2/</li>
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
