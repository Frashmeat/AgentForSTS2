import { useEffect, useState, type FormEvent } from "react";
import { Link, useLocation } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { resendVerification, verifyEmailCode } from "../../shared/api/auth.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function VerifyEmailPage() {
  const location = useLocation();
  const [code, setCode] = useState(() => {
    const routeState = location.state as { code?: string } | null;
    return typeof routeState?.code === "string" ? routeState.code : "";
  });
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  useEffect(() => {
    const routeState = location.state as { code?: string } | null;
    if (typeof routeState?.code === "string") {
      setCode(routeState.code);
    }
  }, [location.state]);

  async function handleVerify(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      await verifyEmailCode(code);
      setFormState(createSuccessAuthFormState("邮箱验证成功，可以直接登录。"));
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "邮箱验证失败"));
    }
  }

  async function handleResend() {
    if (!login.trim() || !password.trim()) {
      setFormState(createErrorAuthFormState("重发验证码需要输入登录名和密码"));
      return;
    }
    setFormState(createSubmittingAuthFormState());
    try {
      const response = await resendVerification({ login, password });
      setCode(response.verification_code);
      setFormState(createSuccessAuthFormState(`新的验证码：${response.verification_code}`));
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "重发失败"));
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="验证邮箱"
      description="注册后先完成邮箱验证，再开始平台模式。"
      actions={<AuthHomeLink />}
      width="narrow"
    >
      <section className="platform-page-card p-8">
        <form className="space-y-4" onSubmit={handleVerify}>
          <label className="block text-sm font-medium text-slate-700">
            验证码
            <input
              className="platform-page-input mt-1"
              value={code}
              onChange={event => setCode(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            重发时使用的登录名
            <input
              className="platform-page-input mt-1"
              value={login}
              onChange={event => setLogin(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            重发时使用的密码
            <input
              type="password"
              className="platform-page-input mt-1"
              value={password}
              onChange={event => setPassword(event.target.value)}
            />
          </label>
          {formState.status !== "idle" && formState.message && (
            <p className={formState.status === "error" ? "text-sm text-rose-600" : "text-sm text-emerald-600"}>
              {formState.message}
            </p>
          )}
          <div className="flex gap-3">
            <button
              type="submit"
              className="platform-page-primary-button flex-1"
              disabled={formState.status === "submitting"}
            >
              验证邮箱
            </button>
            <button
              type="button"
              className="platform-page-secondary-button"
              onClick={() => {
                void handleResend();
              }}
            >
              重发验证码
            </button>
          </div>
        </form>
        <Link to="/auth/login" className="mt-5 inline-flex text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
          返回登录
        </Link>
      </section>
    </PlatformPageShell>
  );
}

export default VerifyEmailPage;
