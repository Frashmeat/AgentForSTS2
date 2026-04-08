interface ExecutionModeDialogProps {
  open: boolean;
  title: string;
  localAvailable: boolean;
  localUnavailableReasons?: string[];
  isAuthenticated: boolean;
  onClose: () => void;
  onChooseLocal: () => void;
  onChooseServer: () => void;
  onGoLogin: () => void;
}

export function ExecutionModeDialog({
  open,
  title,
  localAvailable,
  localUnavailableReasons = [],
  isAuthenticated,
  onClose,
  onChooseLocal,
  onChooseServer,
  onGoLogin,
}: ExecutionModeDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-slate-950/35 px-4">
      <div className="w-full max-w-lg rounded-3xl border border-slate-200 bg-white p-6 shadow-2xl">
        <p className="text-sm uppercase tracking-[0.24em] text-slate-400">Execution Mode</p>
        <h2 className="mt-2 text-2xl font-semibold text-slate-900">{title}</h2>
        <p className="mt-3 text-sm text-slate-500">
          检测到本机配置后，可以继续走本机 BYOK；也可以切到服务器模式，把任务写入平台记录并进入用户中心。
        </p>

        <div className="mt-6 grid gap-3">
          {localAvailable ? (
            <button
              type="button"
              className="rounded-2xl border border-slate-200 px-4 py-4 text-left transition hover:border-amber-300 hover:bg-amber-50/40"
              onClick={onChooseLocal}
            >
              <p className="text-sm font-semibold text-slate-900">本机执行</p>
              <p className="mt-1 text-xs text-slate-500">继续走工作站链路，BYOK / 本地执行不会进入服务器历史。</p>
            </button>
          ) : (
            <div className="rounded-2xl border border-dashed border-slate-200 px-4 py-4 text-sm text-slate-500">
              <p>未检测到当前任务所需的本机 AI 能力，本次仅建议走服务器模式。</p>
              {localUnavailableReasons.length > 0 ? (
                <div className="mt-2 space-y-1 text-xs text-amber-700">
                  {localUnavailableReasons.map((reason) => (
                    <p key={reason}>- {reason}</p>
                  ))}
                </div>
              ) : null}
            </div>
          )}

          <button
            type="button"
            className="rounded-2xl border border-slate-200 px-4 py-4 text-left transition hover:border-emerald-300 hover:bg-emerald-50/40"
            onClick={isAuthenticated ? onChooseServer : onGoLogin}
          >
            <p className="text-sm font-semibold text-slate-900">服务器模式</p>
            <p className="mt-1 text-xs text-slate-500">
              {isAuthenticated
                ? "先创建平台任务，再确认开始。任务会出现在用户中心。"
                : "服务器模式需要登录，点击后会先跳转到登录页。"}
            </p>
          </button>
        </div>

        <div className="mt-6 flex justify-end">
          <button
            type="button"
            className="rounded-xl px-4 py-2 text-sm text-slate-500 transition hover:bg-slate-100 hover:text-slate-700"
            onClick={onClose}
          >
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

export default ExecutionModeDialog;
