import { UsersRound } from "lucide-react";

export function AdminUsersPage() {
  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">用户与额度</h1>
          <p className="mt-1 text-sm text-slate-500">用户资料、额度明细和人工调整能力的扩展位。</p>
        </div>
        <UsersRound className="text-violet-700" size={22} />
      </header>

      <section className="rounded-lg border border-white bg-white/85 p-5 shadow-sm">
        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-lg border border-slate-100 bg-slate-50 px-4 py-3">
            <p className="text-xs font-semibold text-slate-500">用户列表</p>
            <p className="mt-2 text-lg font-semibold text-slate-900">待接入</p>
          </div>
          <div className="rounded-lg border border-slate-100 bg-slate-50 px-4 py-3">
            <p className="text-xs font-semibold text-slate-500">额度明细</p>
            <p className="mt-2 text-lg font-semibold text-slate-900">待接入</p>
          </div>
          <div className="rounded-lg border border-slate-100 bg-slate-50 px-4 py-3">
            <p className="text-xs font-semibold text-slate-500">人工调整</p>
            <p className="mt-2 text-lg font-semibold text-slate-900">待接入</p>
          </div>
        </div>
      </section>

      <section className="rounded-lg border border-amber-100 bg-amber-50 px-4 py-3 text-sm text-amber-700">
        当前后端没有管理员用户列表或额度调整接口，本页不提供不可执行操作。
      </section>
    </div>
  );
}

export default AdminUsersPage;
