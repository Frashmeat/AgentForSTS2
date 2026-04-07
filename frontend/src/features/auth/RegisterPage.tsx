import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { registerWithPassword } from "../../shared/api/auth.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function RegisterPage() {
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      const response = await registerWithPassword({ username, email, password });
      setFormState(createSuccessAuthFormState(`验证码：${response.verification_code}`));
      navigate(`/auth/verify-email?code=${encodeURIComponent(response.verification_code)}`, {
        replace: true,
      });
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "注册失败"));
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="注册"
      description="创建平台账号后，可通过用户中心查看服务器任务记录。"
      actions={<AuthHomeLink />}
      width="narrow"
    >
      <section className="platform-page-card p-8">
        <form className="space-y-4" onSubmit={handleSubmit}>
          <label className="block text-sm font-medium text-slate-700">
            用户名
            <input
              className="platform-page-input mt-1"
              value={username}
              onChange={event => setUsername(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            邮箱
            <input
              className="platform-page-input mt-1"
              value={email}
              onChange={event => setEmail(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            密码
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
          <button
            type="submit"
            className="platform-page-primary-button w-full"
            disabled={formState.status === "submitting"}
          >
            {formState.status === "submitting" ? "提交中..." : "注册"}
          </button>
        </form>
        <p className="mt-5 text-sm text-slate-500">
          已有账号？
          <Link to="/auth/login" className="ml-1 font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
            去登录
          </Link>
        </p>
      </section>
    </PlatformPageShell>
  );
}

export default RegisterPage;
