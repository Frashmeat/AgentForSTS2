import { useEffect, useState } from "react";
import { MinusCircle, PlusCircle, RefreshCw, Search, UsersRound } from "lucide-react";

import {
  adjustAdminUserQuota,
  getAdminUser,
  listAdminUserQuotaLedger,
  listAdminUsers,
  type AdminQuotaLedgerItem,
  type AdminUserDetail,
  type AdminUserListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function formatLedgerType(type: string): string {
  const labels: Record<string, string> = {
    reserve: "执行消耗",
    capture: "执行确认",
    refund: "执行返还",
    admin_grant: "人工增加",
    admin_deduct: "人工扣减",
  };
  return labels[type] ?? type;
}

function formatAnomaly(flag: string): string {
  const labels: Record<string, string> = {
    quota_exhausted: "额度耗尽",
    email_unverified: "邮箱未验证",
    quota_suspended: "额度暂停",
    quota_closed: "额度关闭",
  };
  return labels[flag] ?? flag;
}

function quotaRows(user: AdminUserDetail) {
  return [
    ["剩余次数", user.quota.remaining],
    ["总次数", user.quota.total_limit + user.quota.adjusted_amount],
    ["已使用", user.quota.used_amount],
    ["已返还", user.quota.refunded_amount],
    ["人工调整", user.quota.adjusted_amount],
    ["额度状态", user.quota.status],
  ] as const;
}

export function AdminUsersPage() {
  const [query, setQuery] = useState("");
  const [anomaly, setAnomaly] = useState("");
  const [users, setUsers] = useState<AdminUserListItem[]>([]);
  const [selectedUser, setSelectedUser] = useState<AdminUserDetail | null>(null);
  const [ledger, setLedger] = useState<AdminQuotaLedgerItem[]>([]);
  const [direction, setDirection] = useState<"grant" | "deduct">("grant");
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState("");
  const [loading, setLoading] = useState(false);
  const [adjusting, setAdjusting] = useState(false);
  const [error, setError] = useState("");

  async function loadUsers() {
    setLoading(true);
    setError("");
    try {
      const view = await listAdminUsers({ query: query.trim() || undefined, anomaly: anomaly || undefined, limit: 50 });
      setUsers(view.items);
      if (view.items.length > 0 && selectedUser === null) {
        await selectUser(view.items[0].user_id);
      }
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取用户列表失败");
    } finally {
      setLoading(false);
    }
  }

  async function selectUser(userId: number) {
    setLoading(true);
    setError("");
    try {
      const [detail, ledgerView] = await Promise.all([
        getAdminUser(userId),
        listAdminUserQuotaLedger(userId, undefined, 50),
      ]);
      setSelectedUser(detail);
      setLedger(ledgerView.items);
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取用户详情失败");
    } finally {
      setLoading(false);
    }
  }

  async function submitAdjustment() {
    if (selectedUser === null) {
      setError("请先选择用户。");
      return;
    }
    const numericAmount = Number(amount);
    if (!Number.isInteger(numericAmount) || numericAmount <= 0) {
      setError("请输入正整数次数。");
      return;
    }
    if (!reason.trim()) {
      setError("请填写调整原因。");
      return;
    }
    setAdjusting(true);
    setError("");
    try {
      await adjustAdminUserQuota(selectedUser.user_id, {
        direction,
        amount: numericAmount,
        reason: reason.trim(),
      });
      setAmount("");
      setReason("");
      await selectUser(selectedUser.user_id);
      await loadUsers();
    } catch (adjustError) {
      setError(resolveErrorMessage(adjustError) || "调整额度失败");
    } finally {
      setAdjusting(false);
    }
  }

  useEffect(() => {
    void loadUsers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">用户与额度</h1>
          <p className="mt-1 text-sm text-slate-500">用户资料、固定次数余额、账本和人工调整。</p>
        </div>
        <UsersRound className="text-violet-700" size={22} />
      </header>

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {error}
        </section>
      ) : null}

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="flex flex-wrap items-center gap-2">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="用户编号 / 用户名 / 邮箱"
            className="min-w-64 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          />
          <select
            value={anomaly}
            onChange={(event) => setAnomaly(event.target.value)}
            className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
          >
            <option value="">全部账号</option>
            <option value="quota_exhausted">额度耗尽</option>
            <option value="email_unverified">邮箱未验证</option>
            <option value="quota_suspended">额度暂停</option>
            <option value="quota_closed">额度关闭</option>
          </select>
          <button
            type="button"
            onClick={() => void loadUsers()}
            className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800"
            disabled={loading}
          >
            <Search size={16} />
            <span>查询用户</span>
          </button>
          <button
            type="button"
            onClick={() => void loadUsers()}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-700 transition hover:bg-slate-50"
            disabled={loading}
          >
            <RefreshCw size={16} />
            <span>刷新</span>
          </button>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[1.15fr_0.85fr]">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">用户列表</h2>
          {loading && users.length === 0 ? (
            <p className="mt-3 text-sm text-slate-500">正在读取用户...</p>
          ) : users.length === 0 ? (
            <p className="mt-3 text-sm text-slate-500">当前没有匹配用户。</p>
          ) : (
            <div className="mt-3 overflow-x-auto">
              <table className="min-w-full text-left text-sm">
                <thead className="text-xs text-slate-500">
                  <tr>
                    <th className="px-3 py-2 font-semibold">用户</th>
                    <th className="px-3 py-2 font-semibold">验证</th>
                    <th className="px-3 py-2 font-semibold">角色</th>
                    <th className="px-3 py-2 font-semibold">剩余</th>
                    <th className="px-3 py-2 font-semibold">异常</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {users.map((user) => (
                    <tr
                      key={user.user_id}
                      className="cursor-pointer hover:bg-violet-50/60"
                      onClick={() => void selectUser(user.user_id)}
                    >
                      <td className="px-3 py-2">
                        <p className="font-medium text-slate-900">
                          #{user.user_id} {user.username}
                        </p>
                        <p className="text-xs text-slate-500">{user.email}</p>
                      </td>
                      <td className="px-3 py-2 text-slate-600">{user.email_verified ? "已验证" : "未验证"}</td>
                      <td className="px-3 py-2 text-slate-600">{user.is_admin ? "管理员" : "普通用户"}</td>
                      <td className="px-3 py-2 font-semibold text-slate-900">{user.quota.remaining}</td>
                      <td className="px-3 py-2 text-xs text-slate-500">
                        {user.anomaly_flags.length > 0 ? user.anomaly_flags.map(formatAnomaly).join(" / ") : "无"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <div className="space-y-4">
          <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
            <h2 className="text-base font-semibold text-slate-900">用户详情</h2>
            {selectedUser ? (
              <div className="mt-3 space-y-3">
                <div className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
                  <p className="text-sm font-medium text-slate-900">
                    #{selectedUser.user_id} {selectedUser.username}
                  </p>
                  <p className="mt-1 text-xs text-slate-500">{selectedUser.email}</p>
                  <p className="mt-1 text-xs text-slate-500">创建时间：{formatTime(selectedUser.created_at)}</p>
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  {quotaRows(selectedUser).map(([label, value]) => (
                    <div key={label} className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                      <p className="text-[11px] font-semibold text-slate-400">{label}</p>
                      <p className="mt-1 text-sm font-semibold text-slate-800">{String(value)}</p>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <p className="mt-3 text-sm text-slate-500">选择用户后查看详情。</p>
            )}
          </section>

          <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
            <h2 className="text-base font-semibold text-slate-900">人工调整</h2>
            <div className="mt-3 grid gap-2">
              <div className="inline-flex w-fit rounded-lg border border-slate-200 bg-white p-1">
                <button
                  type="button"
                  onClick={() => setDirection("grant")}
                  className={`inline-flex items-center gap-1 rounded-md px-3 py-1.5 text-sm ${direction === "grant" ? "bg-emerald-50 text-emerald-700" : "text-slate-600"}`}
                >
                  <PlusCircle size={15} />
                  <span>增加</span>
                </button>
                <button
                  type="button"
                  onClick={() => setDirection("deduct")}
                  className={`inline-flex items-center gap-1 rounded-md px-3 py-1.5 text-sm ${direction === "deduct" ? "bg-rose-50 text-rose-700" : "text-slate-600"}`}
                >
                  <MinusCircle size={15} />
                  <span>扣减</span>
                </button>
              </div>
              <input
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
                placeholder="次数"
                className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
              />
              <textarea
                value={reason}
                onChange={(event) => setReason(event.target.value)}
                placeholder="调整原因"
                className="min-h-20 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
              />
              <button
                type="button"
                onClick={() => void submitAdjustment()}
                disabled={adjusting || selectedUser === null}
                className="rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800 disabled:cursor-not-allowed disabled:bg-slate-300"
              >
                提交调整
              </button>
            </div>
          </section>
        </div>
      </section>

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <h2 className="text-base font-semibold text-slate-900">额度账本</h2>
        {selectedUser === null ? (
          <p className="mt-3 text-sm text-slate-500">选择用户后查看账本。</p>
        ) : ledger.length === 0 ? (
          <p className="mt-3 text-sm text-slate-500">当前用户没有额度账本记录。</p>
        ) : (
          <div className="mt-3 overflow-x-auto">
            <table className="min-w-full text-left text-sm">
              <thead className="text-xs text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-semibold">类型</th>
                  <th className="px-3 py-2 font-semibold">次数</th>
                  <th className="px-3 py-2 font-semibold">余额</th>
                  <th className="px-3 py-2 font-semibold">原因</th>
                  <th className="px-3 py-2 font-semibold">执行</th>
                  <th className="px-3 py-2 font-semibold">时间</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {ledger.map((item) => (
                  <tr key={item.ledger_id}>
                    <td className="px-3 py-2 text-slate-700">{formatLedgerType(item.ledger_type)}</td>
                    <td className="px-3 py-2 font-medium text-slate-900">{item.amount}</td>
                    <td className="px-3 py-2 text-slate-600">{item.balance_after}</td>
                    <td className="px-3 py-2 text-slate-600">{item.reason || item.reason_code}</td>
                    <td className="px-3 py-2 text-slate-600">{item.ai_execution_id ?? "无"}</td>
                    <td className="px-3 py-2 text-slate-600">{formatTime(item.created_at)}</td>
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

export default AdminUsersPage;
