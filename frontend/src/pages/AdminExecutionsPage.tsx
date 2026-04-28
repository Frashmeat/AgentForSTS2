import { useState } from "react";
import { ArrowLeft, FileSearch, Search } from "lucide-react";
import { Link } from "react-router-dom";

import { PlatformPageShell } from "../components/platform/PlatformPageShell.tsx";
import {
  getAdminExecution,
  listAdminJobExecutions,
  type AdminExecutionDetail,
  type AdminExecutionListItem,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";

export function AdminExecutionsPage() {
  const [jobId, setJobId] = useState("");
  const [executions, setExecutions] = useState<AdminExecutionListItem[]>([]);
  const [selectedExecution, setSelectedExecution] = useState<AdminExecutionDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadExecutions() {
    const numericJobId = Number(jobId);
    if (!numericJobId) {
      setError("请先输入有效的 job_id。");
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

  const backAction = (
    <Link to="/admin/runtime-audit" className="platform-page-action-link">
      <ArrowLeft size={16} />
      <span>返回审计</span>
    </Link>
  );

  return (
    <PlatformPageShell
      kicker="Admin Executions"
      title="执行记录"
      description="按 job_id 查看平台任务背后的 AI 执行记录，并读取单次执行详情。"
      actions={backAction}
    >
      {error ? <section className="platform-page-card p-4 text-sm text-rose-600">{error}</section> : null}

      <section className="platform-page-card p-6 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">按任务查询</h2>
            <p className="text-sm text-slate-500">输入平台任务的 job_id 查询该任务下的 AI 执行记录。</p>
          </div>
          <FileSearch className="text-slate-500" size={22} />
        </div>
        <div className="flex flex-wrap gap-2">
          <input
            value={jobId}
            onChange={(event) => setJobId(event.target.value)}
            placeholder="job_id"
            className="min-w-48 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          />
          <button type="button" onClick={() => void loadExecutions()} className="platform-page-primary-button" disabled={loading}>
            <Search size={16} />
            <span>查询执行记录</span>
          </button>
        </div>
      </section>

      <section className="platform-page-card p-6 space-y-4">
        <h2 className="text-lg font-semibold text-slate-900">执行列表</h2>
        {loading && executions.length === 0 ? (
          <p className="text-sm text-slate-500">正在读取执行记录...</p>
        ) : executions.length === 0 ? (
          <p className="text-sm text-slate-500">当前没有执行记录。</p>
        ) : (
          <div className="space-y-3">
            {executions.map((execution) => (
              <button
                key={execution.id}
                type="button"
                onClick={() => void loadExecutionDetail(execution.id)}
                className="w-full rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 text-left transition hover:border-amber-200 hover:bg-amber-50/40"
              >
                <div className="flex flex-wrap items-center gap-3">
                  <span className="text-sm font-semibold text-slate-900">execution #{execution.id}</span>
                  <span className="text-xs text-slate-500">job #{execution.job_id}</span>
                  <span className="text-xs text-slate-500">item #{execution.job_item_id}</span>
                  <span className="text-xs text-slate-500">{execution.provider} / {execution.model}</span>
                  <span className="text-xs font-medium text-slate-700">{execution.status}</span>
                </div>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="platform-page-card p-6 space-y-4">
        <h2 className="text-lg font-semibold text-slate-900">执行详情</h2>
        {selectedExecution ? (
          <div className="grid gap-3 md:grid-cols-2">
            {Object.entries(selectedExecution).map(([key, value]) => (
              <div key={key} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
                <p className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">{key}</p>
                <p className="mt-1 break-all text-xs text-slate-700">{String(value ?? "")}</p>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-slate-500">选择一条执行记录后查看详情。</p>
        )}
      </section>
    </PlatformPageShell>
  );
}

export default AdminExecutionsPage;
