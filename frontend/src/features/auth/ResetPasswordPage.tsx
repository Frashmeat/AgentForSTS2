import { useState, type FormEvent } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { resetPasswordWithCode } from "../../shared/api/auth.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
} from "./formModel.ts";

const SESSION_PERSISTENCE_ERROR =
  "密码已重置，但服务端会话未建立。若当前是 hybrid / 跨域部署，请检查 Web 后端 Cookie 的 SameSite、Secure 和 HTTPS 配置。";

export function ResetPasswordPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { refreshSession } = useSession();
  const [code, setCode] = useState(() => searchParams.get("code") ?? "");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      await resetPasswordWithCode(code, password);
      const snapshot = await refreshSession();
      if (!snapshot?.authenticated || snapshot.user === null) {
        throw new Error(SESSION_PERSISTENCE_ERROR);
      }
      navigate("/", { replace: true });
    } catch (error) {
      setFormState(createErrorAuthFormState(resolveErrorMessage(error) || "重置密码失败"));
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="重置密码"
      description="输入收到的重置码，设置新的登录密码。"
      actions={<AuthHomeLink />}
      width="narrow"
    >
      <section className="platform-page-card p-8">
        <form className="space-y-4" onSubmit={handleSubmit}>
          <label className="block text-sm font-medium text-slate-700">
            重置码
            <input
              className="platform-page-input mt-1"
              value={code}
              onChange={event => setCode(event.target.value)}
            />
          </label>
          <label className="block text-sm font-medium text-slate-700">
            新密码
            <input
              type="password"
              className="platform-page-input mt-1"
              value={password}
              onChange={event => setPassword(event.target.value)}
            />
          </label>
          {formState.status === "error" && <p className="text-sm text-rose-600">{formState.message}</p>}
          <button
            type="submit"
            className="platform-page-primary-button w-full"
            disabled={formState.status === "submitting"}
          >
            {formState.status === "submitting" ? "提交中..." : "重置密码"}
          </button>
        </form>
        <Link to="/auth/login" className="mt-5 inline-flex text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
          返回登录
        </Link>
      </section>
    </PlatformPageShell>
  );
}

export default ResetPasswordPage;
