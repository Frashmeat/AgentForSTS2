import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { registerWithPassword } from "../../shared/api/auth.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  type AuthStatusNoticeHandler,
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function RegisterPage({ onStatusNotice }: { onStatusNotice?: AuthStatusNoticeHandler }) {
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
      onStatusNotice?.({
        title: "注册成功",
        message: `验证码：${response.verification_code}`,
        tone: "success",
      });
      navigate("/auth/verify-email", {
        replace: true,
        state: {
          code: response.verification_code,
        },
      });
    } catch (error) {
      const message = resolveErrorMessage(error) || "注册失败";
      setFormState(createErrorAuthFormState(message));
      onStatusNotice?.({
        title: "注册失败",
        message,
        tone: "error",
      });
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
              onChange={(event) => setUsername(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            邮箱
            <input
              className="platform-page-input mt-1"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            密码
            <input
              type="password"
              className="platform-page-input mt-1"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
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
          <Link
            to="/auth/login"
            className="ml-1 font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]"
          >
            去登录
          </Link>
        </p>
      </section>
    </PlatformPageShell>
  );
}

export default RegisterPage;
