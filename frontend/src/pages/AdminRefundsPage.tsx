import { useState } from "react";
import { ArrowLeft, ReceiptText, Search } from "lucide-react";
import { Link } from "react-router-dom";

import { PlatformPageShell } from "../components/platform/PlatformPageShell.tsx";
import { listAdminQuotaRefunds, type AdminQuotaRefundItem } from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";

export function AdminRefundsPage() {
  const [userId, setUserId] = useState("");
  const [refunds, setRefunds] = useState<AdminQuotaRefundItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadRefunds() {
    const normalizedUserId = userId.trim() ? Number(userId) : undefined;
    if (userId.trim() && !normalizedUserId) {
      setError("请输入有效的 user_id。");
      return;
    }
    setLoading(true);
    setError("");
    try {
      setRefunds(await listAdminQuotaRefunds(normalizedUserId));
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取退款记录失败");
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
      kicker="Admin Refunds"
      title="退款记录"
      description="查看平台执行额度返还记录，可按 user_id 筛选。"
      actions={backAction}
    >
      {error ? <section className="platform-page-card p-4 text-sm text-rose-600">{error}</section> : null}

      <section className="platform-page-card p-6 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">筛选条件</h2>
            <p className="text-sm text-slate-500">留空 user_id 时查看全部退款记录。</p>
          </div>
          <ReceiptText className="text-slate-500" size={22} />
        </div>
        <div className="flex flex-wrap gap-2">
          <input
            value={userId}
            onChange={(event) => setUserId(event.target.value)}
            placeholder="user_id"
            className="min-w-48 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          />
          <button type="button" onClick={() => void loadRefunds()} className="platform-page-primary-button" disabled={loading}>
            <Search size={16} />
            <span>查询退款记录</span>
          </button>
        </div>
      </section>

      <section className="platform-page-card p-6 space-y-4">
        <h2 className="text-lg font-semibold text-slate-900">记录列表</h2>
        {loading ? (
          <p className="text-sm text-slate-500">正在读取退款记录...</p>
        ) : refunds.length === 0 ? (
          <p className="text-sm text-slate-500">当前没有退款记录。</p>
        ) : (
          <div className="space-y-3">
            {refunds.map((refund, index) => (
              <article key={`${refund.ai_execution_id}-${index}`} className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="text-sm font-semibold text-slate-900">execution #{refund.ai_execution_id}</span>
                  <span className="text-xs text-slate-500">charge_status: {refund.charge_status}</span>
                  <span className="text-xs text-slate-500">refund_reason: {refund.refund_reason || "未记录"}</span>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </PlatformPageShell>
  );
}

export default AdminRefundsPage;
