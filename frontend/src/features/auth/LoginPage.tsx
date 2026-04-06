import { useState, type FormEvent } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { loginWithPassword } from "../../shared/api/auth.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";
import {
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

export function LoginPage() {
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
      navigate(resolveRedirect(location), { replace: true });
    } catch (error) {
      setFormState(createErrorAuthFormState(resolveErrorMessage(error) || "登录失败"));
    }
  }

  return (
    <section className="mx-auto w-full max-w-md rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">登录</h1>
      <p className="mt-2 text-sm text-slate-500">进入平台链路，查看任务、次数和返还记录。</p>
      <form className="mt-6 space-y-4" onSubmit={handleSubmit}>
        <label className="block text-sm font-medium text-slate-700">
          用户名或邮箱
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={login}
            onChange={event => setLogin(event.target.value)}
          />
        </label>
        <label className="block text-sm font-medium text-slate-700">
          密码
          <input
            type="password"
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={password}
            onChange={event => setPassword(event.target.value)}
          />
        </label>
        {formState.status === "error" && <p className="text-sm text-rose-600">{formState.message}</p>}
        <button
          type="submit"
          className="w-full rounded-xl bg-amber-500 px-4 py-2 font-medium text-white transition hover:bg-amber-600"
          disabled={formState.status === "submitting"}
        >
          {formState.status === "submitting" ? "登录中..." : "登录"}
        </button>
      </form>
      <div className="mt-4 flex items-center justify-between text-sm text-slate-500">
        <Link to="/auth/register" className="text-amber-600 hover:text-amber-700">
          注册账号
        </Link>
        <Link to="/auth/forgot-password" className="text-slate-500 hover:text-slate-700">
          忘记密码
        </Link>
      </div>
    </section>
  );
}

export default LoginPage;
