import { useState } from "react";
import { ReceiptText, Search } from "lucide-react";

import { listAdminQuotaRefunds, type AdminQuotaRefundItem } from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminRefundReason, formatAdminStatus } from "./adminDisplay.ts";

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未返回";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

export function AdminRefundsPage() {
  const [userId, setUserId] = useState("");
  const [refunds, setRefunds] = useState<AdminQuotaRefundItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadRefunds() {
    const normalizedUserId = userId.trim() ? Number(userId) : undefined;
    if (userId.trim() && !normalizedUserId) {
      setError("请输入有效的用户编号。");
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

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">退款记录</h1>
          <p className="mt-1 text-sm text-slate-500">额度返还记录与用户筛选。</p>
        </div>
        <ReceiptText className="text-violet-700" size={22} />
      </header>

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {error}
        </section>
      ) : null}

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="flex flex-wrap gap-2">
          <input
            value={userId}
            onChange={(event) => setUserId(event.target.value)}
            placeholder="用户编号"
            className="min-w-48 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          />
          <button
            type="button"
            onClick={() => void loadRefunds()}
            className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800"
            disabled={loading}
          >
            <Search size={16} />
            <span>查询退款记录</span>
          </button>
        </div>
      </section>

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <h2 className="text-base font-semibold text-slate-900">记录列表</h2>
        {loading ? (
          <p className="mt-3 text-sm text-slate-500">正在读取退款记录...</p>
        ) : refunds.length === 0 ? (
          <p className="mt-3 text-sm text-slate-500">当前没有退款记录。</p>
        ) : (
          <div className="mt-3 overflow-x-auto">
            <table className="min-w-full text-left text-sm">
              <thead className="text-xs text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-semibold">用户</th>
                  <th className="px-3 py-2 font-semibold">执行编号</th>
                  <th className="px-3 py-2 font-semibold">状态</th>
                  <th className="px-3 py-2 font-semibold">原因</th>
                  <th className="px-3 py-2 font-semibold">额度</th>
                  <th className="px-3 py-2 font-semibold">时间</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {refunds.map((refund, index) => (
                  <tr key={`${refund.ai_execution_id}-${index}`}>
                    <td className="px-3 py-2 text-slate-600">{(refund.user_id ?? userId) || "未返回"}</td>
                    <td className="px-3 py-2 font-medium text-slate-900">{refund.ai_execution_id}</td>
                    <td className="px-3 py-2 text-slate-600">{formatAdminStatus(refund.charge_status).label}</td>
                    <td className="px-3 py-2 text-slate-600">{formatAdminRefundReason(refund.refund_reason)}</td>
                    <td className="px-3 py-2 text-slate-600">{refund.quota_amount ?? "未返回"}</td>
                    <td className="px-3 py-2 text-slate-600">{formatTime(refund.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

export default AdminRefundsPage;
