import type { PlatformQuotaView } from "../../shared/api/platform.ts";

export function QuotaCard({ quota }: { quota: PlatformQuotaView }) {
  return (
    <section className="platform-page-card p-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">服务器次数池</h2>
          <p className="mt-1 text-sm text-slate-500">统一展示当前登录用户在平台链路上的剩余额度和返还情况。</p>
        </div>
        <span className="platform-page-pill platform-page-pill-success">
          下次重置 {quota.next_reset_at ?? "待定"}
        </span>
      </div>

      <div className="mt-6 grid gap-4 md:grid-cols-3">
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">日额度</p>
          <p className="mt-2 text-2xl font-semibold text-slate-900">
            {quota.daily_limit - quota.daily_used + quota.refunded} / {quota.daily_limit}
          </p>
        </article>
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">周额度</p>
          <p className="mt-2 text-2xl font-semibold text-slate-900">
            {quota.weekly_limit - quota.weekly_used + quota.refunded} / {quota.weekly_limit}
          </p>
        </article>
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">已返还</p>
          <p className="mt-2 text-2xl font-semibold text-emerald-700">{quota.refunded}</p>
        </article>
      </div>
    </section>
  );
}

export default QuotaCard;
