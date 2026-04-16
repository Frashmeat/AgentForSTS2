import { Link } from "react-router-dom";
import type { UserCenterJobSummary } from "./model.ts";

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

export function HistoryList({ jobs }: { jobs: UserCenterJobSummary[] }) {
  return (
    <section className="platform-page-card p-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">平台任务历史</h2>
          <p className="mt-1 text-sm text-slate-500">这里只展示服务器模式创建的任务，不读取本机恢复快照。</p>
        </div>
        <span className="text-sm text-slate-400">{jobs.length} 条</span>
      </div>

      <div className="mt-6 space-y-3">
        {jobs.length === 0 && (
          <div className="platform-page-empty px-4 py-6 text-sm text-slate-500">
            暂无平台任务记录。用户中心不会读取本机恢复记录或 localStorage 快照。
          </div>
        )}
        {jobs.map(job => (
          <Link
            key={job.id}
            to={`/me/jobs/${job.id}`}
            className="platform-page-list-link block px-4 py-4"
          >
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-sm font-semibold text-slate-900">{job.input_summary || job.job_type}</p>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                  <span className="text-slate-500">{job.job_type} · {renderStatus(job.status)}</span>
                  {job.deferredSummary ? (
                    <span className="rounded-full border border-amber-200 bg-amber-50 px-2 py-0.5 font-medium text-amber-800">
                      {job.deferredSummary.shortLabel}
                    </span>
                  ) : null}
                </div>
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
