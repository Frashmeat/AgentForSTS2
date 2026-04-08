import { ArrowLeft } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { SettingsPanel } from "../components/SettingsPanel.tsx";

function resolveReturnTo(value: string | null): string {
  if (!value || !value.startsWith("/")) {
    return "/";
  }
  return value;
}

export function SettingsPage() {
  const [searchParams] = useSearchParams();
  const returnTo = resolveReturnTo(searchParams.get("returnTo"));

  return (
    <div className="min-h-screen bg-[var(--workspace-bg)] text-slate-800">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
        <div className="space-y-3">
          <Link
            to={returnTo}
            className="inline-flex items-center gap-2 rounded-full border border-slate-200 bg-white/70 px-4 py-2 text-sm font-medium text-slate-700 transition-colors hover:border-violet-300 hover:text-violet-700"
          >
            <ArrowLeft size={15} />
            <span>返回工作区</span>
          </Link>
          <div className="space-y-2">
            <p className="text-xs uppercase tracking-[0.24em] text-[color:var(--workspace-text-muted)]">Workspace Settings</p>
            <div className="space-y-1">
              <h1 className="text-3xl font-semibold text-slate-900">工作区设置</h1>
              <p className="max-w-2xl text-sm text-slate-500">统一管理项目路径、Code Agent 执行方式与图像生成配置。</p>
            </div>
          </div>
        </div>

        <SettingsPanel mode="page" />
      </div>
    </div>
  );
}
