import { useEffect, useState } from "react";
import { ArchiveRestore, CheckCircle2, CloudUpload, FileText, RefreshCcw, Upload } from "lucide-react";

import {
  activateAdminKnowledgePack,
  exportCurrentKnowledgePack,
  listAdminKnowledgePacks,
  rollbackAdminKnowledgePack,
  uploadAdminKnowledgePack,
  type AdminKnowledgePackItem,
  type AdminKnowledgePackListView,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";

function formatEnabled(value?: boolean): string {
  return value ? "有" : "无";
}

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function getPackFiles(pack?: AdminKnowledgePackItem | null): string[] {
  return Array.isArray(pack?.files) ? pack.files : [];
}

function formatFileCount(pack?: AdminKnowledgePackItem | null): string | number {
  if (typeof pack?.file_count === "number") {
    return pack.file_count;
  }
  const files = getPackFiles(pack);
  return files.length > 0 ? files.length : "未记录";
}

function formatStat(value?: number): string | number {
  return typeof value === "number" ? value : "未记录";
}

function PackCapabilityBadge({ label, value }: { label: string; value?: boolean }) {
  return (
    <span
      className={[
        "rounded-full border px-2 py-0.5 text-xs font-medium",
        value ? "border-emerald-200 bg-emerald-50 text-emerald-800" : "border-slate-200 bg-slate-50 text-slate-500",
      ].join(" ")}
    >
      {label}：{formatEnabled(value)}
    </span>
  );
}

function PackSourceStats({ pack }: { pack?: AdminKnowledgePackItem | null }) {
  if (!pack) {
    return null;
  }
  const gameCsCount = pack.game_cs_count;
  const hasKnownMissingGameSource = typeof gameCsCount === "number" && gameCsCount === 0;

  return (
    <div className="space-y-1 text-xs text-slate-500">
      <p>
        源码统计：resources md {formatStat(pack.resource_md_count)} / game cs {formatStat(pack.game_cs_count)} / baselib
        cs {formatStat(pack.baselib_cs_count)}
      </p>
      {hasKnownMissingGameSource ? (
        <p className="text-amber-700">缺少游戏反编译源码，请在工作站更新知识库后重新上传。</p>
      ) : null}
    </div>
  );
}

function PackFileList({ pack, compact = false }: { pack?: AdminKnowledgePackItem | null; compact?: boolean }) {
  const files = getPackFiles(pack);
  if (files.length === 0) {
    return <p className="text-xs text-slate-500">文件列表：未记录</p>;
  }

  return (
    <details className="group text-xs text-slate-600">
      <summary className="inline-flex cursor-pointer list-none items-center gap-1 font-medium text-slate-700 hover:text-violet-700">
        <FileText size={14} />
        <span>文件列表（{files.length}）</span>
      </summary>
      <div
        className={[
          "mt-2 overflow-auto rounded-md border border-slate-200 bg-slate-50",
          compact ? "max-h-36 min-w-64" : "max-h-48",
        ].join(" ")}
      >
        <ul className="divide-y divide-slate-200">
          {files.map((path) => (
            <li key={path} className="break-all px-2 py-1 font-mono text-[11px] leading-5 text-slate-700">
              {path}
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}

function ActivePackSummary({ view }: { view: AdminKnowledgePackListView | null }) {
  const activePack = view?.active_pack;
  return (
    <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
      <div className="flex items-center gap-2">
        <CheckCircle2 size={18} className="text-emerald-700" />
        <h2 className="text-base font-semibold text-slate-900">当前激活包</h2>
      </div>
      {activePack ? (
        <div className="mt-4 space-y-3">
          <div>
            <p className="text-sm font-semibold text-slate-900">{activePack.label || activePack.pack_id}</p>
            <p className="mt-1 break-all text-xs text-slate-500">{activePack.pack_id}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <PackCapabilityBadge label="resources" value={activePack.has_resources} />
            <PackCapabilityBadge label="game" value={activePack.has_game} />
            <PackCapabilityBadge label="baselib" value={activePack.has_baselib} />
          </div>
          <p className="text-xs text-slate-500">文件数：{formatFileCount(activePack)}</p>
          <PackSourceStats pack={activePack} />
          <PackFileList pack={activePack} />
        </div>
      ) : (
        <p className="mt-4 text-sm text-slate-500">当前未激活知识库包，运行时会回退到 runtime/内置知识库。</p>
      )}
    </section>
  );
}

export function AdminKnowledgePacksPage() {
  const [view, setView] = useState<AdminKnowledgePackListView | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [label, setLabel] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [activateAfterLocalUpload, setActivateAfterLocalUpload] = useState(false);

  async function loadData() {
    setLoading(true);
    setError("");
    try {
      setView(await listAdminKnowledgePacks());
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取知识库包失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadData();
  }, []);

  async function submitUpload() {
    if (file === null) {
      setError("请选择知识库 zip 包。");
      return;
    }
    setSaving(true);
    setError("");
    setMessage("");
    try {
      await uploadAdminKnowledgePack(file, label.trim());
      setMessage("知识库包已上传。");
      setFile(null);
      setLabel("");
      await loadData();
    } catch (uploadError) {
      setError(resolveErrorMessage(uploadError) || "上传知识库包失败");
    } finally {
      setSaving(false);
    }
  }

  async function uploadFromLocalWorkstation() {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const exported = await exportCurrentKnowledgePack();
      const displayLabel = label.trim() || `本机知识库 ${new Date().toLocaleString("zh-CN", { hour12: false })}`;
      const pack = await uploadAdminKnowledgePack(exported.blob, displayLabel, exported.fileName);
      if (activateAfterLocalUpload) {
        await activateAdminKnowledgePack(pack.pack_id);
      }
      setMessage(activateAfterLocalUpload ? "本机工作站知识库已上传并激活。" : "本机工作站知识库已上传。");
      setLabel("");
      await loadData();
    } catch (uploadError) {
      setError(resolveErrorMessage(uploadError) || "从本机工作站上传知识库失败");
    } finally {
      setSaving(false);
    }
  }

  async function runAction(action: () => Promise<unknown>, successMessage: string) {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      await action();
      setMessage(successMessage);
      await loadData();
    } catch (actionError) {
      setError(resolveErrorMessage(actionError) || "知识库包操作失败");
    } finally {
      setSaving(false);
    }
  }

  const items: AdminKnowledgePackItem[] = view?.items ?? [];

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">知识库包</h1>
          <p className="mt-1 text-sm text-slate-500">上传、激活和回滚服务器生成使用的 STS2 知识资源。</p>
        </div>
        <button
          type="button"
          onClick={() => void loadData()}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新知识库"}</span>
        </button>
      </header>

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {error}
        </section>
      ) : null}
      {message ? (
        <section className="rounded-lg border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
          {message}
        </section>
      ) : null}

      <section className="grid gap-4 xl:grid-cols-[0.85fr_1.15fr]">
        <div className="space-y-4">
          <ActivePackSummary view={view} />

          <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
            <div className="flex items-center gap-2">
              <Upload size={18} className="text-violet-700" />
              <h2 className="text-base font-semibold text-slate-900">上传知识库包</h2>
            </div>
            <div className="mt-4 space-y-3">
              <label className="space-y-1 text-sm text-slate-600">
                <span>显示名</span>
                <input
                  value={label}
                  onChange={(event) => setLabel(event.target.value)}
                  className="w-full rounded-lg border border-slate-200 px-3 py-2"
                  placeholder="例如 STS2 2026-04"
                />
              </label>
              <label className="space-y-1 text-sm text-slate-600">
                <span>Zip 包</span>
                <input
                  type="file"
                  accept=".zip,application/zip"
                  onChange={(event) => setFile(event.target.files?.[0] ?? null)}
                  className="w-full rounded-lg border border-slate-200 bg-white px-3 py-2"
                />
              </label>
              <label className="flex items-center gap-2 text-xs text-slate-600">
                <input
                  type="checkbox"
                  checked={activateAfterLocalUpload}
                  onChange={(event) => setActivateAfterLocalUpload(event.target.checked)}
                  className="h-4 w-4 rounded border-slate-300"
                />
                <span>从本机工作站上传后立即激活</span>
              </label>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void submitUpload()}
                  disabled={saving || file === null}
                  className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800 disabled:cursor-not-allowed disabled:bg-slate-300"
                >
                  <Upload size={16} />
                  <span>{saving ? "处理中" : "上传知识库包"}</span>
                </button>
                <button
                  type="button"
                  onClick={() => void uploadFromLocalWorkstation()}
                  disabled={saving}
                  className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-700 transition hover:border-violet-200 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <CloudUpload size={16} />
                  <span>{saving ? "处理中" : "从本机工作站上传"}</span>
                </button>
              </div>
              <p className="text-xs leading-5 text-slate-500">
                本机上传会先连接管理员电脑上的 Workstation，导出当前 runtime/knowledge，再上传到服务器。
              </p>
            </div>
          </section>
        </div>

        <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold text-slate-900">已上传包</h2>
            <button
              type="button"
              onClick={() => void runAction(rollbackAdminKnowledgePack, "知识库包已回滚。")}
              className="inline-flex items-center gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm font-medium text-amber-800 transition hover:bg-amber-100 disabled:opacity-50"
              disabled={saving}
            >
              <ArchiveRestore size={16} />
              <span>回滚</span>
            </button>
          </div>

          {items.length === 0 ? (
            <p className="rounded-lg border border-dashed border-slate-200 px-4 py-6 text-sm text-slate-500">
              当前还没有上传知识库包。
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="min-w-full text-left text-sm">
                <thead className="text-xs text-slate-500">
                  <tr>
                    <th className="px-3 py-2 font-semibold">名称</th>
                    <th className="px-3 py-2 font-semibold">内容</th>
                    <th className="px-3 py-2 font-semibold">文件数</th>
                    <th className="px-3 py-2 font-semibold">上传时间</th>
                    <th className="px-3 py-2 font-semibold">操作</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {items.map((pack) => (
                    <tr key={pack.pack_id}>
                      <td className="px-3 py-2">
                        <p className="font-medium text-slate-900">{pack.label || pack.pack_id}</p>
                        <p className="mt-1 break-all text-xs text-slate-500">{pack.pack_id}</p>
                        {pack.active ? <p className="mt-1 text-xs font-medium text-emerald-700">当前激活</p> : null}
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex flex-wrap gap-1">
                          <PackCapabilityBadge label="resources" value={pack.has_resources} />
                          <PackCapabilityBadge label="game" value={pack.has_game} />
                          <PackCapabilityBadge label="baselib" value={pack.has_baselib} />
                        </div>
                      </td>
                      <td className="px-3 py-2 text-slate-600">
                        <div className="space-y-2">
                          <p>{formatFileCount(pack)}</p>
                          <PackSourceStats pack={pack} />
                          <PackFileList pack={pack} compact />
                        </div>
                      </td>
                      <td className="px-3 py-2 text-slate-600">{formatTime(pack.uploaded_at ?? pack.created_at)}</td>
                      <td className="px-3 py-2">
                        <button
                          type="button"
                          onClick={() =>
                            void runAction(() => activateAdminKnowledgePack(pack.pack_id), "知识库包已激活。")
                          }
                          disabled={saving || pack.active}
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          激活
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </section>
    </div>
  );
}

export default AdminKnowledgePacksPage;
