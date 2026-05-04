import { useEffect, useState } from "react";
import { ArchiveRestore, CheckCircle2, CloudUpload, FileText, RefreshCcw, Trash2, Upload } from "lucide-react";

import {
  activateAdminKnowledgePack,
  deleteAdminKnowledgePack,
  exportCurrentKnowledgePack,
  listAdminKnowledgePacks,
  rollbackAdminKnowledgePack,
  uploadAdminKnowledgePack,
  type AdminKnowledgePackItem,
  type AdminKnowledgePackListView,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import {
  platformErrorNoticeDetails,
  platformErrorNoticeTitle,
  toPlatformErrorView,
} from "../../shared/platform/index.ts";
import { useAdminLayoutContext } from "./AdminLayout.tsx";

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
  const missingResourceFiles = Array.isArray(pack.missing_resource_files) ? pack.missing_resource_files : [];
  const hasKnownMissingBaselib = pack.has_baselib_file === false || pack.has_baselib === false;
  const hasKnownMissingRequiredResources = pack.has_required_resources === false || missingResourceFiles.length > 0;

  return (
    <div className="space-y-1 text-xs text-slate-500">
      <p>
        源码统计：resources md {formatStat(pack.resource_md_count)} / game cs {formatStat(pack.game_cs_count)} / baselib
        cs {formatStat(pack.baselib_cs_count)}
      </p>
      {typeof pack.required_resource_count === "number" && typeof pack.required_resource_total === "number" ? (
        <p>
          必需文档：{pack.required_resource_count}/{pack.required_resource_total}
        </p>
      ) : null}
      {hasKnownMissingGameSource ? (
        <p className="text-amber-700">缺少完整游戏反编译源码，服务器会拒绝上传或激活此类知识库包。</p>
      ) : null}
      {hasKnownMissingBaselib ? (
        <p className="text-amber-700">缺少 BaseLib 反编译源码：baselib/BaseLib.decompiled.cs。</p>
      ) : null}
      {hasKnownMissingRequiredResources ? (
        <p className="break-all text-amber-700">
          缺少必需资源文档{missingResourceFiles.length > 0 ? `：${missingResourceFiles.join("，")}` : "。"}
        </p>
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

function isIncompleteKnowledgePackError(error: unknown): boolean {
  const message = resolveErrorMessage(error, "");
  return message.includes("知识库包不完整") || message.includes("知识库包缺少");
}

type KnowledgePackOperation = "manual-upload" | "local-upload" | "activate" | "rollback" | "delete";

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
            <PackCapabilityBadge label="resources" value={activePack.has_required_resources ?? activePack.has_resources} />
            <PackCapabilityBadge label="game" value={activePack.has_game} />
            <PackCapabilityBadge label="baselib" value={activePack.has_baselib_file ?? activePack.has_baselib} />
          </div>
          <p className="text-xs text-slate-500">文件数：{formatFileCount(activePack)}</p>
          <PackSourceStats pack={activePack} />
          <PackFileList pack={activePack} />
        </div>
      ) : (
        <p className="mt-4 text-sm text-slate-500">当前 Web runtime 没有激活完整知识库包。</p>
      )}
    </section>
  );
}

export function AdminKnowledgePacksPage() {
  const { onStatusNotice, onConfirm } = useAdminLayoutContext();
  const [view, setView] = useState<AdminKnowledgePackListView | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [label, setLabel] = useState("");
  const [loading, setLoading] = useState(false);
  const [activeOperation, setActiveOperation] = useState<KnowledgePackOperation | null>(null);
  const [operationStage, setOperationStage] = useState("");
  const [activateAfterLocalUpload, setActivateAfterLocalUpload] = useState(false);
  const saving = activeOperation !== null;

  function showNotice(title: string, message: string, tone: "info" | "success" | "warning" | "error" = "info") {
    onStatusNotice?.({ title, message, tone });
  }

  function resolveKnowledgePackError(error: unknown, fallback: string): string {
    const message = resolveErrorMessage(error, fallback);
    if (isIncompleteKnowledgePackError(error)) {
      return `${message} 请先在本机 Workstation 设置页执行“更新知识库”，确认 runtime/knowledge 同时包含 game/**/*.cs、baselib/BaseLib.decompiled.cs 和 resources/sts2/*.md 后再上传。`;
    }
    return message;
  }

  function showLocalWorkstationError(error: unknown) {
    const message = resolveKnowledgePackError(error, "从本机工作站上传知识库失败");
    const view = toPlatformErrorView(
      {
        schema_version: "platform_error.v1",
        origin: "local_workstation",
        runtime_surface: "browser",
        component: "knowledge_pack",
        operation: "export_current_knowledge_pack",
        category: "network_error",
        reason_code: "local_workstation_export_failed",
        message,
        developer_message: message,
        retryable: true,
        log_hint: {
          primary: "local_workstation_log",
          secondary: "web_backend_log",
        },
        diagnostic: {
          raw_error: message,
        },
      },
      message,
    );
    onStatusNotice?.({
      title: platformErrorNoticeTitle(view),
      message: view.message,
      details: platformErrorNoticeDetails(view),
      tone: "error",
    });
  }

  async function loadData() {
    setLoading(true);
    try {
      setView(await listAdminKnowledgePacks());
    } catch (loadError) {
      showNotice("读取知识库包失败", resolveErrorMessage(loadError, "读取知识库包失败"), "error");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadData();
  }, []);

  async function submitUpload() {
    if (file === null) {
      showNotice("请选择文件", "请选择要上传的知识库 zip 包。", "warning");
      return;
    }
    setActiveOperation("manual-upload");
    setOperationStage("上传到 Web");
    showNotice("正在上传知识库包", "正在上传 zip 包到 Web 后端，请等待上传和完整性校验完成。", "info");
    try {
      await uploadAdminKnowledgePack(file, label.trim());
      setOperationStage("刷新服务器状态");
      showNotice("知识库包已上传", "服务器已收到知识库包，请在列表中确认完整性后激活。", "success");
      setFile(null);
      setLabel("");
      await loadData();
    } catch (uploadError) {
      showNotice("上传知识库包失败", resolveKnowledgePackError(uploadError, "上传知识库包失败"), "error");
    } finally {
      setActiveOperation(null);
      setOperationStage("");
    }
  }

  async function uploadFromLocalWorkstation() {
    setActiveOperation("local-upload");
    setOperationStage("导出本机知识库");
    showNotice("正在导出本机知识库", "正在连接本机 Workstation 并导出当前 runtime/knowledge。", "info");
    try {
      const exported = await exportCurrentKnowledgePack();
      setOperationStage("上传到 Web");
      showNotice("正在上传本机知识库", "本机知识库已导出，正在上传到 Web 后端。", "info");
      const displayLabel = label.trim() || `本机知识库 ${new Date().toLocaleString("zh-CN", { hour12: false })}`;
      const pack = await uploadAdminKnowledgePack(exported.blob, displayLabel, exported.fileName);
      if (activateAfterLocalUpload) {
        setOperationStage("安装并激活");
        showNotice("正在激活知识库包", "Web 后端正在安装刚上传的知识库包，并切换服务器知识库真源。", "info");
        await activateAdminKnowledgePack(pack.pack_id);
      }
      setOperationStage("刷新服务器状态");
      showNotice(
        activateAfterLocalUpload ? "本机知识库已上传并激活" : "本机知识库已上传",
        activateAfterLocalUpload
          ? "服务器已使用刚上传的知识库包作为当前激活包。"
          : "服务器已保存本机工作站导出的知识库包。",
        "success",
      );
      setLabel("");
      await loadData();
    } catch (uploadError) {
      showLocalWorkstationError(uploadError);
    } finally {
      setActiveOperation(null);
      setOperationStage("");
    }
  }

  async function runAction(
    action: () => Promise<unknown>,
    successMessage: string,
    stage = "执行操作",
    operation: KnowledgePackOperation = "activate",
  ) {
    setActiveOperation(operation);
    setOperationStage(stage);
    showNotice("正在处理知识库包", stage, "info");
    try {
      await action();
      setOperationStage("刷新服务器状态");
      showNotice("知识库包操作完成", successMessage, "success");
      await loadData();
    } catch (actionError) {
      showNotice("知识库包操作失败", resolveErrorMessage(actionError, "知识库包操作失败"), "error");
    } finally {
      setActiveOperation(null);
      setOperationStage("");
    }
  }

  async function deletePack(pack: AdminKnowledgePackItem) {
    const labelText = pack.label || pack.pack_id;
    const message = `确定删除知识库包“${labelText}”吗？${
      pack.active ? " 当前激活包删除后会自动回退或清空激活状态。" : ""
    }`;
    if (!onConfirm) {
      await runAction(() => deleteAdminKnowledgePack(pack.pack_id), "知识库包已删除。", "删除知识库包", "delete");
      return;
    }
    onConfirm({
      title: "删除知识库包",
      message,
      confirmLabel: "删除",
      cancelLabel: "取消",
      tone: "warning",
      onConfirm: () => {
        void runAction(() => deleteAdminKnowledgePack(pack.pack_id), "知识库包已删除。", "删除知识库包", "delete");
      },
    });
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
                  <span>{activeOperation === "manual-upload" && operationStage ? operationStage : "上传知识库包"}</span>
                </button>
                <button
                  type="button"
                  onClick={() => void uploadFromLocalWorkstation()}
                  disabled={saving}
                  className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-700 transition hover:border-violet-200 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <CloudUpload size={16} />
                  <span>
                    {activeOperation === "local-upload" && operationStage ? operationStage : "从本机工作站上传"}
                  </span>
                </button>
              </div>
              <p className="text-xs leading-5 text-slate-500">
                本机上传会先连接管理员电脑上的 Workstation，导出当前 runtime/knowledge，再上传到服务器；导出包必须包含完整
                game/**/*.cs、baselib/BaseLib.decompiled.cs 和 resources/sts2/*.md，请先在 Workstation 更新并补齐知识库。
              </p>
            </div>
          </section>
        </div>

        <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold text-slate-900">已上传包</h2>
            <button
              type="button"
              onClick={() => void runAction(rollbackAdminKnowledgePack, "知识库包已回滚。", "回滚知识库包", "rollback")}
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
                          <PackCapabilityBadge label="resources" value={pack.has_required_resources ?? pack.has_resources} />
                          <PackCapabilityBadge label="game" value={pack.has_game} />
                          <PackCapabilityBadge label="baselib" value={pack.has_baselib_file ?? pack.has_baselib} />
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
                            void runAction(
                              () => activateAdminKnowledgePack(pack.pack_id),
                              "知识库包已激活。",
                              "安装并激活",
                              "activate",
                            )
                          }
                          disabled={saving || pack.active}
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          激活
                        </button>
                        <button
                          type="button"
                          onClick={() => void deletePack(pack)}
                          disabled={saving}
                          className="ml-2 inline-flex items-center gap-1 rounded-lg border border-rose-200 px-3 py-1.5 text-xs text-rose-700 transition hover:bg-rose-50 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          <Trash2 size={13} />
                          删除
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
