import { useState } from "react";
import { FileSearch, Search } from "lucide-react";

import {
  getAdminExecution,
  listAdminJobExecutions,
  type AdminExecutionDetail,
  type AdminExecutionListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminProvider, formatAdminStatus } from "./adminDisplay.ts";

function detailRows(execution: AdminExecutionDetail) {
  return [
    ["执行编号", execution.id],
    ["任务编号", execution.job_id],
    ["子任务", execution.job_item_id],
    ["服务商", formatAdminProvider(execution.provider)],
    ["模型", execution.model],
    ["状态", formatAdminStatus(execution.status).label],
    ["请求标识", execution.request_idempotency_key],
    ["输入摘要", execution.input_summary],
    ["结果摘要", execution.result_summary],
    ["错误摘要", execution.error_summary],
  ] as const;
}

function statusClass(status: string): string {
  const tone = formatAdminStatus(status).tone;
  switch (tone) {
    case "success":
      return "border-emerald-100 bg-emerald-50 text-emerald-700";
    case "warning":
      return "border-amber-100 bg-amber-50 text-amber-700";
    case "danger":
      return "border-rose-100 bg-rose-50 text-rose-700";
    case "info":
      return "border-blue-100 bg-blue-50 text-blue-700";
    default:
      return "border-slate-100 bg-slate-50 text-slate-600";
  }
}

export function AdminExecutionsPage() {
  const [jobId, setJobId] = useState("");
  const [executions, setExecutions] = useState<AdminExecutionListItem[]>([]);
  const [selectedExecution, setSelectedExecution] = useState<AdminExecutionDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadExecutions() {
    const numericJobId = Number(jobId);
    if (!numericJobId) {
      setError("请先输入有效的任务编号。");
      return;
    }
    setLoading(true);
    setError("");
    setSelectedExecution(null);
    try {
      setExecutions(await listAdminJobExecutions(numericJobId));
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取执行记录失败");
    } finally {
      setLoading(false);
    }
  }

  async function loadExecutionDetail(executionId: number) {
    setLoading(true);
    setError("");
    try {
      setSelectedExecution(await getAdminExecution(executionId));
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取执行详情失败");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">执行记录</h1>
          <p className="mt-1 text-sm text-slate-500">按任务编号查询 AI 执行记录。</p>
        </div>
        <FileSearch className="text-violet-700" size={22} />
      </header>

      {error ? <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section> : null}

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="flex flex-wrap gap-2">
          <input
            value={jobId}
            onChange={(event) => setJobId(event.target.value)}
            placeholder="任务编号"
            className="min-w-48 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          />
          <button
            type="button"
            onClick={() => void loadExecutions()}
            className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800"
            disabled={loading}
          >
            <Search size={16} />
            <span>查询执行记录</span>
          </button>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[1.1fr_0.9fr]">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">执行列表</h2>
          {loading && executions.length === 0 ? (
            <p className="mt-3 text-sm text-slate-500">正在读取执行记录...</p>
          ) : executions.length === 0 ? (
            <p className="mt-3 text-sm text-slate-500">当前没有执行记录。</p>
          ) : (
            <div className="mt-3 overflow-x-auto">
              <table className="min-w-full text-left text-sm">
                <thead className="text-xs text-slate-500">
                  <tr>
                    <th className="px-3 py-2 font-semibold">执行编号</th>
                    <th className="px-3 py-2 font-semibold">任务编号</th>
                    <th className="px-3 py-2 font-semibold">子任务</th>
                    <th className="px-3 py-2 font-semibold">服务商</th>
                    <th className="px-3 py-2 font-semibold">状态</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {executions.map((execution) => {
                    const status = formatAdminStatus(execution.status);
                    return (
                      <tr
                        key={execution.id}
                        className="cursor-pointer hover:bg-violet-50/60"
                        onClick={() => void loadExecutionDetail(execution.id)}
                      >
                        <td className="px-3 py-2 font-medium text-slate-900">{execution.id}</td>
                        <td className="px-3 py-2 text-slate-600">{execution.job_id}</td>
                        <td className="px-3 py-2 text-slate-600">{execution.job_item_id}</td>
                        <td className="px-3 py-2 text-slate-600">{formatAdminProvider(execution.provider)} / {execution.model}</td>
                        <td className="px-3 py-2">
                          <span className={`rounded-md border px-2 py-1 text-xs font-medium ${statusClass(execution.status)}`}>{status.label}</span>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">执行详情</h2>
          {selectedExecution ? (
            <div className="mt-3 space-y-3">
              <div className="grid gap-2 sm:grid-cols-2">
                {detailRows(selectedExecution).map(([label, value]) => (
                  <div key={label} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
                    <p className="text-[11px] font-semibold text-slate-400">{label}</p>
                    <p className="mt-1 break-all text-xs text-slate-700">{String(value ?? "未记录")}</p>
                  </div>
                ))}
              </div>
              <details>
                <summary className="cursor-pointer text-xs font-medium text-slate-500">技术信息</summary>
                <div className="mt-2 grid gap-2 sm:grid-cols-2">
                  <div className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                    <p className="text-[11px] font-semibold text-slate-400">step_protocol_version</p>
                    <p className="mt-1 break-all text-xs text-slate-600">{selectedExecution.step_protocol_version || "未记录"}</p>
                  </div>
                  <div className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                    <p className="text-[11px] font-semibold text-slate-400">result_schema_version</p>
                    <p className="mt-1 break-all text-xs text-slate-600">{selectedExecution.result_schema_version || "未记录"}</p>
                  </div>
                </div>
              </details>
            </div>
          ) : (
            <p className="mt-3 text-sm text-slate-500">选择一条执行记录后查看详情。</p>
          )}
        </div>
      </section>
    </div>
  );
}

export default AdminExecutionsPage;
