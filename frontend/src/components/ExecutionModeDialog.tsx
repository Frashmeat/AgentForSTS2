import type { PlatformExecutionProfile } from "../shared/api/platform.ts";

interface ExecutionModeDialogProps {
  open: boolean;
  title: string;
  localAvailable: boolean;
  localUnavailableReasons?: string[];
  serverUnsupportedReasons?: string[];
  isAuthenticated: boolean;
  serverProfiles: PlatformExecutionProfile[];
  serverProfilesLoading: boolean;
  serverProfilesError: string | null;
  serverSelectionNotice: string | null;
  selectedServerProfileId: number | null;
  rememberServerProfile: boolean;
  onClose: () => void;
  onChooseLocal: () => void;
  onChooseServer: () => void;
  onGoLogin: () => void;
  onSelectServerProfile: (profileId: number) => void;
  onRememberServerProfileChange: (value: boolean) => void;
  onReloadServerProfiles: () => void;
}

export function ExecutionModeDialog({
  open,
  title,
  localAvailable,
  localUnavailableReasons = [],
  serverUnsupportedReasons = [],
  isAuthenticated,
  serverProfiles,
  serverProfilesLoading,
  serverProfilesError,
  serverSelectionNotice,
  selectedServerProfileId,
  rememberServerProfile,
  onClose,
  onChooseLocal,
  onChooseServer,
  onGoLogin,
  onSelectServerProfile,
  onRememberServerProfileChange,
  onReloadServerProfiles,
}: ExecutionModeDialogProps) {
  if (!open) {
    return null;
  }

  const hasAvailableServerProfile = serverProfiles.some((profile) => profile.available);
  const serverActionDisabled = isAuthenticated && (
    serverUnsupportedReasons.length > 0 ||
    serverProfilesLoading ||
    !hasAvailableServerProfile ||
    selectedServerProfileId === null ||
    serverProfiles.every((profile) => profile.id !== selectedServerProfileId || !profile.available)
  );

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-slate-950/35 px-4">
      <div className="w-full max-w-2xl rounded-3xl border border-slate-200 bg-white p-6 shadow-2xl">
        <p className="text-sm uppercase tracking-[0.24em] text-slate-400">Execution Mode</p>
        <h2 className="mt-2 text-2xl font-semibold text-slate-900">{title}</h2>
        <p className="mt-3 text-sm text-slate-500">
          检测到本机配置后，可以继续走本机 BYOK；也可以切到服务器模式，先选平台提供的执行配置，再把任务写入平台记录并进入用户中心。
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

          <div className="rounded-2xl border border-slate-200 px-4 py-4 text-left">
            <p className="text-sm font-semibold text-slate-900">服务器模式</p>
            <p className="mt-1 text-xs text-slate-500">
              {isAuthenticated
                ? "先选择一个可用的服务器执行配置，再创建平台任务。"
                : "服务器模式需要登录，点击后会先跳转到登录页。"}
            </p>

            {isAuthenticated ? (
              <>
                {serverUnsupportedReasons.length > 0 ? (
                  <div className="mt-3 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
                    {serverUnsupportedReasons.map((reason) => (
                      <p key={reason}>- {reason}</p>
                    ))}
                  </div>
                ) : serverProfilesLoading ? (
                  <p className="mt-3 text-xs text-slate-500">正在读取服务器执行配置…</p>
                ) : serverProfilesError ? (
                  <div className="mt-3 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
                    <p>{serverProfilesError}</p>
                    <button
                      type="button"
                      className="mt-2 rounded-lg border border-amber-300 px-2.5 py-1 text-xs font-medium text-amber-800 transition hover:bg-amber-100"
                      onClick={onReloadServerProfiles}
                    >
                      重试读取
                    </button>
                  </div>
                ) : (
                  <>
                    {serverSelectionNotice ? (
                      <div className="mt-3 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
                        {serverSelectionNotice}
                      </div>
                    ) : null}
                    <div className="mt-3 space-y-2">
                      {serverProfiles.map((profile) => {
                        const selected = selectedServerProfileId === profile.id;
                        return (
                          <button
                            key={profile.id}
                            type="button"
                            disabled={!profile.available}
                            onClick={() => onSelectServerProfile(profile.id)}
                            className={[
                              "w-full rounded-xl border px-3 py-3 text-left transition",
                              selected
                                ? "border-emerald-400 bg-emerald-50"
                                : "border-slate-200 hover:border-emerald-200 hover:bg-emerald-50/30",
                              !profile.available ? "cursor-not-allowed opacity-60" : "",
                            ].join(" ")}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div>
                                <p className="text-sm font-medium text-slate-900">{profile.display_name}</p>
                                <p className="mt-1 text-xs text-slate-500">{profile.description}</p>
                              </div>
                              <div className="shrink-0 space-y-1 text-right">
                                {profile.recommended ? (
                                  <p className="text-[11px] font-medium text-emerald-700">推荐</p>
                                ) : null}
                                <p className={`text-[11px] font-medium ${profile.available ? "text-slate-500" : "text-amber-700"}`}>
                                  {profile.available ? "可用" : "当前不可用"}
                                </p>
                              </div>
                            </div>
                          </button>
                        );
                      })}
                    </div>

                    {hasAvailableServerProfile ? (
                      <label className="mt-3 flex items-center gap-2 text-xs text-slate-500">
                        <input
                          type="checkbox"
                          checked={rememberServerProfile}
                          onChange={(event) => onRememberServerProfileChange(event.target.checked)}
                        />
                        将本次选择设为默认服务器配置
                      </label>
                    ) : (
                      <p className="mt-3 text-xs text-amber-700">当前没有健康可用的服务器执行配置，暂时无法开始服务器任务。</p>
                    )}
                  </>
                )}

                <button
                  type="button"
                  className="mt-4 rounded-xl border border-emerald-300 bg-emerald-50 px-4 py-2 text-sm font-medium text-emerald-800 transition hover:bg-emerald-100 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={serverActionDisabled}
                  onClick={onChooseServer}
                >
                  创建服务器任务
                </button>
              </>
            ) : (
              <button
                type="button"
                className="mt-4 rounded-xl border border-emerald-300 bg-emerald-50 px-4 py-2 text-sm font-medium text-emerald-800 transition hover:bg-emerald-100"
                onClick={onGoLogin}
              >
                登录后继续
              </button>
            )}
          </div>
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
