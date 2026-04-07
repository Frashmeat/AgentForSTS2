import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { requestPasswordReset } from "../../shared/api/auth.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function ForgotPasswordPage() {
  const navigate = useNavigate();
  const [login, setLogin] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      const response = await requestPasswordReset(login);
      setFormState(createSuccessAuthFormState(`重置码：${response.reset_code}`));
      navigate(`/auth/reset-password?code=${encodeURIComponent(response.reset_code)}`, {
        replace: true,
      });
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "找回密码失败"));
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="找回密码"
      description="输入登录名或邮箱，获取重置密码所需的验证码。"
      actions={<AuthHomeLink />}
      width="narrow"
    >
      <section className="platform-page-card p-8">
        <form className="space-y-4" onSubmit={handleSubmit}>
          <label className="block text-sm font-medium text-slate-700">
            登录名
            <input
              className="platform-page-input mt-1"
              value={login}
              onChange={event => setLogin(event.target.value)}
            />
          </label>
          {formState.status !== "idle" && formState.message && (
            <p className={formState.status === "error" ? "text-sm text-rose-600" : "text-sm text-emerald-600"}>
              {formState.message}
            </p>
          )}
          <button
            type="submit"
            className="platform-page-primary-button w-full"
            disabled={formState.status === "submitting"}
          >
            获取重置码
          </button>
        </form>
        <Link to="/auth/login" className="mt-5 inline-flex text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
          返回登录
        </Link>
      </section>
    </PlatformPageShell>
  );
}

export default ForgotPasswordPage;
