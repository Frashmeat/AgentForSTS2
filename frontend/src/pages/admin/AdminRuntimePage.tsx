import { useEffect, useState } from "react";
import { RefreshCcw } from "lucide-react";

import {
  getAdminWorkstationRuntimeLogs,
  getAdminWorkstationRuntimeStatus,
  loadPlatformQueueWorkerStatus,
  type AdminWorkstationCapabilities,
  type AdminWorkstationRuntimeLogTail,
  type AdminWorkstationRuntimeStatus,
  type PlatformQueueWorkerStatus,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminEventType } from "./adminDisplay.ts";

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function formatUnavailableReason(reason?: string): string {
  switch (reason) {
    case "not_platform_runtime":
      return "当前运行环境不是平台 Web 服务。";
    case "worker_not_configured":
      return "当前未配置平台队列 Worker。";
    default:
      return reason || "当前未暴露队列 Worker 运行态。";
  }
}

function RuntimeMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-lg border border-white bg-white/85 px-4 py-3 shadow-sm">
      <p className="text-xs font-semibold text-slate-500">{label}</p>
      <p className="mt-2 break-all text-lg font-semibold text-slate-900">{value}</p>
    </div>
  );
}

function formatEnabled(value?: boolean): string {
  if (value === true) {
    return "可用";
  }
  if (value === false) {
    return "不可用";
  }
  return "未记录";
}

function formatRuntimeEnabled(value?: boolean): string {
  if (value === true) {
    return "已开启";
  }
  if (value === false) {
    return "未开启";
  }
  return "未记录";
}

function formatWorkstationUnavailableReason(status: AdminWorkstationRuntimeStatus | null): string {
  const reason = String(status?.reason || status?.capabilities?.reason || status?.last_error || "").trim();
  if (!reason) {
    return "当前未读取到 Web 托管 Workstation 运行态。";
  }
  switch (reason) {
    case "workstation_runtime_manager_not_registered":
      return "当前 Web 运行环境没有注册 Workstation 托管管理器。";
    case "workstation_control_token_missing":
      return "Workstation 控制 token 未配置，无法读取内部能力。";
    default:
      return reason;
  }
}

function CapabilityMetric({ label, value }: { label: string; value?: boolean }) {
  const available = value === true;
  const unavailable = value === false;
  return (
    <div
      className={[
        "rounded-lg border px-4 py-3",
        available ? "border-emerald-100 bg-emerald-50/80" : "",
        unavailable ? "border-slate-200 bg-slate-50" : "",
        !available && !unavailable ? "border-white bg-white/85" : "",
      ].join(" ")}
    >
      <p className="text-xs font-semibold text-slate-500">{label}</p>
      <p className={["mt-2 text-base font-semibold", available ? "text-emerald-800" : "text-slate-800"].join(" ")}>
        {formatEnabled(value)}
      </p>
    </div>
  );
}

function WorkstationCapabilitiesPanel({ capabilities }: { capabilities?: AdminWorkstationCapabilities | null }) {
  return (
    <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
      <h2 className="text-base font-semibold text-slate-900">服务器生成能力</h2>
      <div className="mt-3 grid gap-3 sm:grid-cols-2">
        <CapabilityMetric label="文本生成" value={capabilities?.generation?.text_generation_available} />
        <CapabilityMetric label="代码生成" value={capabilities?.generation?.code_generation_available} />
        <CapabilityMetric label="服务器构建" value={capabilities?.build?.server_build_supported} />
        <CapabilityMetric label="服务器部署" value={capabilities?.deploy?.server_deploy_supported} />
        <CapabilityMetric label="内置 STS2 知识" value={capabilities?.knowledge?.embedded_sts2_guidance} />
        <CapabilityMetric label="激活知识库包" value={capabilities?.knowledge?.knowledge_pack_active} />
      </div>
      {capabilities?.knowledge?.active_knowledge_pack_id ? (
        <p className="mt-3 break-all text-xs text-slate-500">
          当前知识库包：{capabilities.knowledge.active_knowledge_pack_id}
        </p>
      ) : null}
      {capabilities?.reason ? <p className="mt-3 text-xs text-amber-700">{capabilities.reason}</p> : null}
    </div>
  );
}

type RuntimeLogStream = "stderr" | "stdout";

function formatBytes(value?: number): string {
  const bytes = Number(value || 0);
  if (bytes >= 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

function WorkstationRuntimeLogPanel() {
  const [stream, setStream] = useState<RuntimeLogStream>("stderr");
  const [log, setLog] = useState<AdminWorkstationRuntimeLogTail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadLog(nextStream = stream) {
    setLoading(true);
    setError("");
    try {
      setLog(await getAdminWorkstationRuntimeLogs(nextStream));
    } catch (reason) {
      setLog(null);
      setError(resolveErrorMessage(reason) || "读取 Web Workstation 日志失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadLog("stderr");
  }, []);

  const emptyMessage = log?.exists === false ? "日志文件尚未生成。" : "日志为空。";

  return (
    <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-slate-900">Web Workstation 日志</h2>
          <p className="mt-1 text-xs text-slate-500">管理员诊断用，仅读取固定 stdout / stderr 日志尾部。</p>
        </div>
        <div className="flex items-center gap-2">
          <div className="inline-flex overflow-hidden rounded-lg border border-slate-200 bg-slate-50 p-1">
            {(["stderr", "stdout"] as RuntimeLogStream[]).map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => {
                  setStream(option);
                  void loadLog(option);
                }}
                className={[
                  "px-3 py-1.5 text-xs font-medium transition",
                  stream === option
                    ? "rounded-md bg-white text-slate-950 shadow-sm"
                    : "text-slate-500 hover:text-slate-900",
                ].join(" ")}
              >
                {option}
              </button>
            ))}
          </div>
          <button
            type="button"
            onClick={() => {
              void loadLog();
            }}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
            disabled={loading}
          >
            <RefreshCcw size={16} />
            <span>{loading ? "读取中" : "刷新日志"}</span>
          </button>
        </div>
      </div>

      {error ? (
        <div className="mt-3 rounded-lg border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>
      ) : null}

      {log ? (
        <div className="mt-3 grid gap-2 text-xs text-slate-500 sm:grid-cols-3">
          <span className="break-all">路径：{log.path || "未记录"}</span>
          <span>大小：{formatBytes(log.size_bytes)}</span>
          <span>{log.truncated ? `已截取最近 ${formatBytes(log.tail_bytes)}` : "未截断"}</span>
        </div>
      ) : null}

      <pre className="mt-3 max-h-96 min-h-40 overflow-auto rounded-lg border border-slate-200 bg-slate-950 p-4 font-mono text-xs leading-5 text-slate-100">
        {log?.content?.trim() ? log.content : emptyMessage}
      </pre>
    </section>
  );
}

export function AdminRuntimePage() {
  const [status, setStatus] = useState<PlatformQueueWorkerStatus | null>(null);
  const [workstationStatus, setWorkstationStatus] = useState<AdminWorkstationRuntimeStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadStatus() {
    setLoading(true);
    setError("");
    const [queueResult, workstationResult] = await Promise.allSettled([
      loadPlatformQueueWorkerStatus(),
      getAdminWorkstationRuntimeStatus(),
    ]);

    if (queueResult.status === "fulfilled") {
      setStatus(queueResult.value);
    } else {
      setStatus(null);
      setError(resolveErrorMessage(queueResult.reason) || "读取队列运行状态失败");
    }

    if (workstationResult.status === "fulfilled") {
      setWorkstationStatus(workstationResult.value);
    } else {
      setWorkstationStatus(null);
      setError(
        (previous) => previous || resolveErrorMessage(workstationResult.reason) || "读取 Workstation 运行状态失败",
      );
    }

    setLoading(false);
  }

  useEffect(() => {
    void loadStatus();
  }, []);

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">运行状态</h1>
          <p className="mt-1 text-sm text-slate-500">队列 Worker、Web 托管 Workstation 和服务器生成能力。</p>
        </div>
        <button
          type="button"
          onClick={() => {
            void loadStatus();
          }}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新状态"}</span>
        </button>
      </header>

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {error}
        </section>
      ) : null}

      {!status?.available ? (
        <section className="rounded-lg border border-amber-100 bg-amber-50 px-4 py-3 text-sm text-amber-700">
          {formatUnavailableReason(status?.reason)}
        </section>
      ) : null}

      <section className="grid gap-3 md:grid-cols-4">
        <RuntimeMetric label="当前实例" value={status?.owner_id || "未记录"} />
        <RuntimeMetric label="当前角色" value={status?.is_leader ? "调度者" : "非调度者"} />
        <RuntimeMetric label="调度世代" value={status?.leader_epoch ?? "未记录"} />
        <RuntimeMetric label="最近 Tick" value={formatTime(status?.last_tick_at)} />
      </section>

      {!workstationStatus?.available ? (
        <section className="rounded-lg border border-amber-100 bg-amber-50 px-4 py-3 text-sm text-amber-700">
          {formatWorkstationUnavailableReason(workstationStatus)}
        </section>
      ) : null}

      <section className="grid gap-4 xl:grid-cols-[0.95fr_1.05fr]">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">Web 托管 Workstation</h2>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <RuntimeMetric label="进程状态" value={workstationStatus?.running ? "运行中" : "未运行"} />
            <RuntimeMetric label="托管方式" value={workstationStatus?.managed ? "当前 Web 托管" : "未托管"} />
            <RuntimeMetric label="自动启动" value={formatRuntimeEnabled(workstationStatus?.auto_start)} />
            <RuntimeMetric label="PID" value={workstationStatus?.pid ?? "未记录"} />
            <RuntimeMetric label="内部地址" value={workstationStatus?.workstation_url || "未记录"} />
            <RuntimeMetric label="Token Env" value={workstationStatus?.control_token_env || "未记录"} />
            <RuntimeMetric label="stdout 日志" value={workstationStatus?.stdout_log_path || "未记录"} />
            <RuntimeMetric label="stderr 日志" value={workstationStatus?.stderr_log_path || "未记录"} />
          </div>
          {workstationStatus?.last_error ? (
            <p className="mt-3 break-all text-xs text-rose-600">{workstationStatus.last_error}</p>
          ) : null}
        </div>

        <WorkstationCapabilitiesPanel capabilities={workstationStatus?.capabilities} />
      </section>

      <WorkstationRuntimeLogPanel />

      <section className="grid gap-4 xl:grid-cols-2">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">当前调度权</h2>
          {status?.current_leader ? (
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <RuntimeMetric label="Owner" value={status.current_leader.owner_id} />
              <RuntimeMetric label="Epoch" value={status.current_leader.leader_epoch ?? "未记录"} />
              <RuntimeMetric label="获得时间" value={formatTime(status.current_leader.claimed_at)} />
              <RuntimeMetric label="续租时间" value={formatTime(status.current_leader.renewed_at)} />
              <RuntimeMetric label="过期时间" value={formatTime(status.current_leader.expires_at)} />
            </div>
          ) : (
            <p className="mt-3 text-sm text-slate-500">当前没有读取到有效的调度权租约。</p>
          )}
        </div>

        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">最近运行事件</h2>
          {status?.recent_leader_events?.length ? (
            <div className="mt-3 space-y-2">
              {[...status.recent_leader_events].slice(0, 8).map((event, index) => (
                <article
                  key={`${event.event_type}-${event.occurred_at}-${index}`}
                  className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2"
                >
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="font-medium text-slate-900">{formatAdminEventType(event.event_type)}</span>
                    <span className="text-xs text-slate-500">{formatTime(event.occurred_at)}</span>
                    <span className="text-xs text-slate-500">epoch {event.leader_epoch ?? "未记录"}</span>
                  </div>
                  {event.detail ? <p className="mt-1 text-xs text-slate-500">{event.detail}</p> : null}
                </article>
              ))}
            </div>
          ) : (
            <p className="mt-3 text-sm text-slate-500">当前还没有记录到运行事件。</p>
          )}
        </div>
      </section>
    </div>
  );
}

export default AdminRuntimePage;
