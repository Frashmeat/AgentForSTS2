import type { PlatformQuotaView } from "../../shared/api/platform.ts";

function resolveRemainingQuota(quota: PlatformQuotaView): number {
  const dailyRemaining = quota.daily_limit - quota.daily_used + quota.refunded;
  const weeklyRemaining = quota.weekly_limit - quota.weekly_used + quota.refunded;
  return Math.max(dailyRemaining, weeklyRemaining, 0);
}

export function QuotaCard({ quota }: { quota: PlatformQuotaView }) {
  const remaining = resolveRemainingQuota(quota);

  return (
    <section className="platform-page-card p-6">
      <div>
        <div>
          <h2 className="text-lg font-semibold text-slate-900">服务器次数池</h2>
          <p className="mt-1 text-sm text-slate-500">统一展示当前登录用户在平台链路上的剩余次数和返还情况。</p>
        </div>
      </div>

      <div className="mt-6 grid gap-4 md:grid-cols-2">
        <article className="platform-page-subcard p-4">
          <p className="text-sm text-slate-500">剩余次数</p>
          <p className="mt-2 text-2xl font-semibold text-slate-900">
            {remaining}
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
