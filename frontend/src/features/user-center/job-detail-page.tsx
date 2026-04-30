import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { AlertTriangle, Clock3, Download, House, Upload } from "lucide-react";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { pickAppPath } from "../../shared/api/config.ts";
import { getMyArtifactDownloadUrl, getMyJob, listMyJobEvents, listMyJobItems } from "../../shared/api/me.ts";
import { importProjectPackage } from "../../shared/api/workflow.ts";
import type {
  PlatformArtifactSummary,
  PlatformJobDetail,
  PlatformJobEventSummary,
  PlatformJobItemSummary,
} from "../../shared/api/platform.ts";
import { readDeferredExecutionNotice } from "../../shared/deferredExecution.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { RefundSummary } from "./refundSummary.tsx";
import { formatExecutionProfileText } from "./executionProfileText.ts";
import { renderJobItemStatus, renderJobStatus } from "./statusText.ts";
import { useSession } from "../../shared/session/hooks.ts";

function formatOccurredAt(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString("zh-CN", { hour12: false });
}

function renderArtifactTypeLabel(value: string) {
  switch (value) {
    case "build_output":
      return "构建产物";
    case "deployed_output":
      return "部署产物";
    case "source_project":
      return "项目源码包";
    default:
      return value || "未知产物";
  }
}

function resolveArtifactLocationLabel(artifact: PlatformArtifactSummary) {
  if (artifact.artifact_type === "source_project") {
    return "交付方式";
  }
  if (artifact.artifact_type === "deployed_output") {
    return "部署位置";
  }
  return "产物路径";
}

function isDownloadableArtifact(artifact: PlatformArtifactSummary) {
  return artifact.storage_provider === "server_workspace" && artifact.artifact_type === "source_project";
}

function hasSourceProjectArtifact(artifacts: PlatformArtifactSummary[] | undefined) {
  return (artifacts ?? []).some((artifact) => artifact.artifact_type === "source_project");
}

function renderArtifactLocationValue(artifact: PlatformArtifactSummary) {
  if (artifact.artifact_type === "source_project") {
    return "服务器生成项目包";
  }
  return artifact.object_key;
}

function renderDeliveryStateLabel(value: string | undefined, sourcePackageGenerated = false) {
  if (sourcePackageGenerated) {
    return "项目包已生成";
  }
  const normalized = String(value ?? "").trim();
  if (normalized === "deployed") {
    return "已部署";
  }
  if (normalized === "built") {
    return "已构建";
  }
  return "未标记";
}

function readPayloadText(payload: Record<string, unknown>, key: string) {
  const value = payload[key];
  return typeof value === "string" ? value.trim() : "";
}

function renderEventMessage(event: PlatformJobEventSummary) {
  if (event.event_type === "ai_execution.deferred") {
    return String(event.payload.reason_message ?? "当前任务尚未进入真实服务器执行。");
  }
  if (event.event_type === "workstation.step.started" || event.event_type === "workstation.step.finished") {
    const message = readPayloadText(event.payload, "message");
    const phase = readPayloadText(event.payload, "phase");
    const stepType = readPayloadText(event.payload, "step_type");
    const sequence = event.payload.sequence;
    const detailParts = [
      phase ? `阶段：${phase}` : "",
      stepType ? `步骤：${stepType}` : "",
      typeof sequence === "number" ? `序号：${sequence}` : "",
    ].filter(Boolean);
    return [message || "服务器生成步骤已更新", detailParts.join(" · ")].filter(Boolean).join("。");
  }
  switch (event.event_type) {
    case "job.created":
      return "平台任务已创建。";
    case "job.queued":
      return "平台任务已进入服务器队列。";
    case "job.cancel_requested":
      return "已请求取消任务。";
    case "ai_execution.started":
      return "服务器执行已开始。";
    case "ai_execution.finished":
      return "服务器执行已结束。";
    case "ai_execution.retry_scheduled":
      return "服务器执行已安排重试。";
    case "job.partial_blocked_by_quota":
      return "部分任务因额度不足未执行。";
    default:
      return "任务状态已更新。";
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("读取项目包失败"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const marker = ",";
      const markerIndex = result.indexOf(marker);
      resolve(markerIndex >= 0 ? result.slice(markerIndex + marker.length) : result);
    };
    reader.readAsDataURL(blob);
  });
}

async function downloadArtifactAsBase64(artifactId: number): Promise<string> {
  const response = await fetch(getMyArtifactDownloadUrl(artifactId), {
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return blobToBase64(await response.blob());
}

export function UserCenterJobDetailPage() {
  const { jobId } = useParams();
  const { isAuthenticated, isLoading } = useSession();
  const [detail, setDetail] = useState<PlatformJobDetail | null>(null);
  const [items, setItems] = useState<PlatformJobItemSummary[]>([]);
  const [events, setEvents] = useState<PlatformJobEventSummary[]>([]);
  const [error, setError] = useState("");
  const [actionMessage, setActionMessage] = useState("");
  const [actionError, setActionError] = useState("");
  const [importingArtifactId, setImportingArtifactId] = useState<number | null>(null);
  const navigationActions = (
    <>
      <Link to="/me" className="platform-page-action-link">
        返回用户中心
      </Link>
      <Link to="/" className="platform-page-action-link">
        <House size={16} />
        <span>返回首页</span>
      </Link>
    </>
  );

  useEffect(() => {
    if (!isAuthenticated || !jobId) {
      return;
    }
    let cancelled = false;
    void Promise.all([getMyJob(Number(jobId)), listMyJobItems(Number(jobId)), listMyJobEvents(Number(jobId))])
      .then(([detailResult, itemResults, eventResults]) => {
        if (!cancelled) {
          setDetail(detailResult);
          setItems(itemResults);
          setEvents(eventResults);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "加载任务详情失败");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isAuthenticated, jobId]);

  async function importArtifactToLocalWorkstation(artifact: PlatformArtifactSummary) {
    setActionMessage("");
    setActionError("");
    setImportingArtifactId(artifact.id);
    try {
      const picked = await pickAppPath({
        kind: "directory",
        title: "选择本机项目导入目录",
      });
      const targetDir = String(picked.path ?? "").trim();
      if (!targetDir) {
        setActionMessage("已取消选择导入目录。");
        return;
      }
      const packageBase64 = await downloadArtifactAsBase64(artifact.id);
      const result = await importProjectPackage({
        package_base64: packageBase64,
        file_name: artifact.file_name || "server-project.zip",
        target_dir: targetDir,
      });
      setActionMessage(`项目包已导入本机工作站：${result.project_path}`);
    } catch (importError) {
      setActionError(resolveErrorMessage(importError) || "导入项目包失败");
    } finally {
      setImportingArtifactId(null);
    }
  }

  if (isLoading) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="任务详情"
        description="正在恢复会话并读取平台任务详情。"
        actions={navigationActions}
      >
        <section className="platform-page-card p-8 text-sm text-slate-500">正在恢复会话...</section>
      </PlatformPageShell>
    );
  }

  if (!isAuthenticated) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="任务详情"
        description="登录后可查看服务器模式任务的执行结果与返还信息。"
        actions={navigationActions}
      >
        <section className="platform-page-card p-8">
          <Link
            to="/auth/login"
            className="text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]"
          >
            登录后查看任务详情
          </Link>
        </section>
      </PlatformPageShell>
    );
  }

  if (error) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="任务详情"
        description="平台任务详情加载失败。"
        actions={navigationActions}
      >
        <section className="platform-page-card p-8 text-sm text-rose-600">{error}</section>
      </PlatformPageShell>
    );
  }

  if (detail === null) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="任务详情"
        description="正在读取任务状态、返还摘要与子项列表。"
        actions={navigationActions}
      >
        <section className="platform-page-card p-8 text-sm text-slate-500">任务详情加载中...</section>
      </PlatformPageShell>
    );
  }

  const deferredNotice = readDeferredExecutionNotice(events);
  const executionProfileText = formatExecutionProfileText(detail);
  const sourcePackageGenerated = hasSourceProjectArtifact(detail.artifacts);

  return (
    <PlatformPageShell
      kicker="User Center"
      title={detail.input_summary || detail.job_type}
      description={`${detail.job_type} · ${renderJobStatus(detail.status)}`}
      actions={navigationActions}
    >
      <RefundSummary detail={detail} />

      {actionError ? (
        <section className="platform-page-card border-rose-100 bg-rose-50 p-4 text-sm text-rose-700">
          {actionError}
        </section>
      ) : null}
      {actionMessage ? (
        <section className="platform-page-card border-emerald-100 bg-emerald-50 p-4 text-sm text-emerald-700">
          {actionMessage}
        </section>
      ) : null}

      {executionProfileText ? (
        <section className="platform-page-card p-6">
          <h2 className="text-lg font-semibold text-slate-900">执行配置</h2>
          <div className="mt-4 space-y-2 text-sm text-slate-600">
            <p>
              任务选择：<span className="font-medium text-slate-900">{executionProfileText}</span>
            </p>
            {detail.selected_execution_profile_id ? (
              <p>
                执行配置 ID：<span className="font-medium text-slate-900">{detail.selected_execution_profile_id}</span>
              </p>
            ) : null}
            {detail.delivery_state ? (
              <p>
                交付状态：
                <span className="font-medium text-slate-900">
                  {renderDeliveryStateLabel(detail.delivery_state, sourcePackageGenerated)}
                </span>
              </p>
            ) : null}
            {sourcePackageGenerated ? (
              <p>
                后续操作：<span className="font-medium text-slate-900">下载项目包后在本机工作站构建和部署</span>
              </p>
            ) : null}
          </div>
        </section>
      ) : null}

      {deferredNotice ? (
        <section className="platform-page-card p-6">
          <div className="flex items-start gap-3 rounded-[22px] border border-amber-200 bg-amber-50/90 px-4 py-4 text-amber-950">
            <AlertTriangle size={18} className="mt-0.5 shrink-0 text-amber-600" />
            <div className="space-y-2">
              <div>
                <p className="text-sm font-semibold">{deferredNotice.summary.title}</p>
                <p className="mt-1 text-sm text-amber-900/80">{deferredNotice.summary.description}</p>
              </div>
              <div className="text-xs text-amber-900/70">
                <p>事件时间：{formatOccurredAt(deferredNotice.event.occurred_at)}</p>
                <p>后端说明：{deferredNotice.summary.detail}</p>
              </div>
            </div>
          </div>
        </section>
      ) : null}

      <section className="platform-page-card p-6">
        <h2 className="text-lg font-semibold text-slate-900">子项列表</h2>
        <div className="mt-6 space-y-3">
          {items.map((item) => (
            <article key={item.id} className="platform-page-subcard px-4 py-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold text-slate-900">
                    #{item.item_index + 1} · {item.item_type}
                  </p>
                  <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
                    <span className="text-slate-500">{renderJobItemStatus(item.status)}</span>
                    {item.delivery_state ? (
                      <span className="rounded-full border border-slate-200 bg-slate-50 px-2 py-0.5 font-medium text-slate-700">
                        {renderDeliveryStateLabel(item.delivery_state, sourcePackageGenerated)}
                      </span>
                    ) : null}
                  </div>
                </div>
                <div className="max-w-xs text-right text-xs text-slate-500">
                  <p>{item.result_summary || "无结果摘要"}</p>
                  <p>{item.error_summary || "无错误信息"}</p>
                </div>
              </div>
            </article>
          ))}
        </div>
      </section>

      {detail.artifacts && detail.artifacts.length > 0 ? (
        <section className="platform-page-card p-6">
          <h2 className="text-lg font-semibold text-slate-900">交付产物</h2>
          <div className="mt-6 space-y-3">
            {detail.artifacts.map((artifact) => (
              <article key={artifact.id} className="platform-page-subcard px-4 py-4">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-sm font-semibold text-slate-900">{artifact.file_name || "未命名产物"}</p>
                    <p className="mt-1 text-xs text-slate-500">{renderArtifactTypeLabel(artifact.artifact_type)}</p>
                  </div>
                  <div className="max-w-xs text-right text-xs text-slate-500">
                    <p>{artifact.result_summary || "无产物摘要"}</p>
                    {artifact.object_key ? (
                      <p className="mt-1 break-all">
                        {resolveArtifactLocationLabel(artifact)}：{renderArtifactLocationValue(artifact)}
                      </p>
                    ) : null}
                    {artifact.storage_provider ? <p className="mt-1">来源：{artifact.storage_provider}</p> : null}
                    {isDownloadableArtifact(artifact) ? (
                      <div className="mt-3 flex flex-wrap justify-end gap-2">
                        <a
                          className="inline-flex items-center justify-center gap-2 rounded-md bg-[var(--workspace-accent)] px-3 py-2 text-xs font-semibold text-white transition hover:bg-[var(--workspace-accent-strong)]"
                          href={getMyArtifactDownloadUrl(artifact.id)}
                        >
                          <Download size={14} />
                          <span>下载项目包</span>
                        </a>
                        <button
                          type="button"
                          className="inline-flex items-center justify-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-semibold text-slate-700 transition hover:border-[var(--workspace-accent)] hover:text-[var(--workspace-accent-strong)] disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={importingArtifactId === artifact.id}
                          onClick={() => {
                            void importArtifactToLocalWorkstation(artifact);
                          }}
                        >
                          <Upload size={14} />
                          <span>{importingArtifactId === artifact.id ? "导入中" : "导入本机工作站"}</span>
                        </button>
                      </div>
                    ) : null}
                  </div>
                </div>
              </article>
            ))}
          </div>
        </section>
      ) : null}

      <section className="platform-page-card p-6">
        <div className="flex items-center gap-2">
          <Clock3 size={16} className="text-[var(--workspace-accent)]" />
          <h2 className="text-lg font-semibold text-slate-900">执行事件</h2>
        </div>
        <div className="mt-6 space-y-3">
          {events.length === 0 ? (
            <div className="platform-page-empty px-4 py-6 text-sm text-slate-500">当前任务还没有可展示的执行事件。</div>
          ) : (
            events.map((event) => (
              <article key={event.event_id} className="platform-page-subcard px-4 py-4">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="text-sm font-semibold text-slate-900">{event.event_type}</p>
                    <p className="mt-1 text-xs text-slate-500">{formatOccurredAt(event.occurred_at)}</p>
                  </div>
                  <div className="max-w-xl text-right text-xs text-slate-500">
                    <p>{renderEventMessage(event)}</p>
                  </div>
                </div>
              </article>
            ))
          )}
        </div>
      </section>
    </PlatformPageShell>
  );
}

export default UserCenterJobDetailPage;
