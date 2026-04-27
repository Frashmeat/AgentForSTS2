import { useState, type FormEvent } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { loginWithPassword } from "../../shared/api/auth.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  type AuthStatusNoticeHandler,
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
} from "./formModel.ts";

const SESSION_PERSISTENCE_ERROR =
  "登录响应已返回，但服务端会话未建立。若当前是 hybrid / 跨域部署，请检查 Web 后端 Cookie 的 SameSite、Secure 和 HTTPS 配置。";

function resolveRedirect(location: ReturnType<typeof useLocation>): string {
  const state = location.state as { redirectTo?: string } | null;
  return state?.redirectTo ?? "/";
}

export function LoginPage({ onStatusNotice }: { onStatusNotice?: AuthStatusNoticeHandler }) {
  const navigate = useNavigate();
  const location = useLocation();
  const { refreshSession } = useSession();
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      await loginWithPassword({ login, password });
      const snapshot = await refreshSession();
      if (!snapshot?.authenticated || snapshot.user === null) {
        throw new Error(SESSION_PERSISTENCE_ERROR);
      }
      onStatusNotice?.({
        title: "登录成功",
        message: "已恢复平台账号会话。",
        tone: "success",
      });
      navigate(resolveRedirect(location), { replace: true });
    } catch (error) {
      const message = resolveErrorMessage(error) || "登录失败";
      setFormState(createErrorAuthFormState(message));
      onStatusNotice?.({
        title: "登录失败",
        message,
        tone: "error",
      });
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="登录"
      description="进入平台链路，查看任务、次数和返还记录。"
      actions={<AuthHomeLink />}
      width="narrow"
    >
      <section className="platform-page-card p-8">
        <form className="space-y-4" onSubmit={handleSubmit}>
          <label className="block text-sm font-medium text-slate-700">
            用户名或邮箱
            <input
              className="platform-page-input mt-1"
              value={login}
              onChange={event => setLogin(event.target.value)}
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
          <button
            type="submit"
            className="platform-page-primary-button w-full"
            disabled={formState.status === "submitting"}
          >
            {formState.status === "submitting" ? "登录中..." : "登录"}
          </button>
        </form>
        <div className="mt-5 flex items-center justify-between text-sm text-slate-500">
          <Link to="/auth/register" className="font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
            注册账号
          </Link>
          <Link to="/auth/forgot-password" className="transition hover:text-[var(--workspace-accent-strong)]">
            忘记密码
          </Link>
        </div>
      </section>
    </PlatformPageShell>
  );
}

export default LoginPage;
