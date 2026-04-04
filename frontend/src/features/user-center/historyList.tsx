import { Link } from "react-router-dom";
import type { PlatformJobSummary } from "../../shared/api/platform.ts";

function renderStatus(status: string) {
  switch (status) {
    case "succeeded":
      return "已完成";
    case "running":
      return "执行中";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

export function HistoryList({ jobs }: { jobs: PlatformJobSummary[] }) {
  return (
    <section className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">平台任务历史</h2>
          <p className="mt-1 text-sm text-slate-500">这里只展示服务器模式创建的任务，不读取本机恢复快照。</p>
        </div>
        <span className="text-sm text-slate-400">{jobs.length} 条</span>
      </div>

      <div className="mt-6 space-y-3">
        {jobs.length === 0 && (
          <div className="rounded-2xl border border-dashed border-slate-200 px-4 py-6 text-sm text-slate-500">
            暂无平台任务记录。用户中心不会读取本机恢复记录或 localStorage 快照。
          </div>
        )}
        {jobs.map(job => (
          <Link
            key={job.id}
            to={`/me/jobs/${job.id}`}
            className="block rounded-2xl border border-slate-200 px-4 py-4 transition hover:border-amber-300 hover:bg-amber-50/40"
          >
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-sm font-semibold text-slate-900">{job.input_summary || job.job_type}</p>
                <p className="mt-1 text-xs text-slate-500">
                  {job.job_type} · {renderStatus(job.status)}
                </p>
              </div>
              <div className="text-right text-xs text-slate-500">
                <p>原始扣减 {job.original_deducted ?? 0}</p>
                <p>返还 {job.refunded_amount ?? 0}</p>
                <p>净消耗 {job.net_consumed ?? 0}</p>
              </div>
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}

export default HistoryList;
