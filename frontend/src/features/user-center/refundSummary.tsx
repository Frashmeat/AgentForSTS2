import type { PlatformJobDetail } from "../../shared/api/platform.ts";

export function RefundSummary({ detail }: { detail: PlatformJobDetail }) {
  return (
    <section className="platform-page-card p-6">
      <h2 className="text-lg font-semibold text-slate-900">返还摘要</h2>
      <p className="mt-1 text-sm text-slate-500">展示服务器模式下本次任务的原始扣减、返还次数和净消耗。</p>
      <div className="mt-6 grid gap-4 md:grid-cols-4">
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">原始扣减</p>
          <p className="mt-2 text-2xl font-semibold text-slate-900">{detail.original_deducted ?? 0}</p>
        </article>
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">返还次数</p>
          <p className="mt-2 text-2xl font-semibold text-emerald-700">{detail.refunded_amount ?? 0}</p>
        </article>
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">净消耗</p>
          <p className="mt-2 text-2xl font-semibold text-slate-900">{detail.net_consumed ?? 0}</p>
        </article>
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">返还原因</p>
          <p className="mt-2 text-sm font-medium text-slate-700">{detail.refund_reason_summary || "无"}</p>
        </article>
      </div>
    </section>
  );
}

export default RefundSummary;
