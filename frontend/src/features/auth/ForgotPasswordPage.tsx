import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { requestPasswordReset } from "../../shared/api/auth.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { AuthHomeLink } from "./AuthHomeLink.tsx";
import {
  type AuthStatusNoticeHandler,
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function ForgotPasswordPage({ onStatusNotice }: { onStatusNotice?: AuthStatusNoticeHandler }) {
  const navigate = useNavigate();
  const [login, setLogin] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      await requestPasswordReset(login);
      setFormState(createSuccessAuthFormState("重置请求已提交，请粘贴收到的重置码继续设置新密码。"));
      onStatusNotice?.({
        title: "重置请求已提交",
        message: "请粘贴收到的重置码继续设置新密码。",
        tone: "success",
      });
      navigate("/auth/reset-password", {
        replace: true,
      });
    } catch (error) {
      const message = resolveErrorMessage(error) || "找回密码失败";
      setFormState(createErrorAuthFormState(message));
      onStatusNotice?.({
        title: "找回密码失败",
        message,
        tone: "error",
      });
    }
  }

  return (
    <PlatformPageShell
      kicker="Platform Access"
      title="找回密码"
      description="输入登录名或邮箱，提交重置请求后再手动输入收到的重置码。"
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
              onChange={(event) => setLogin(event.target.value)}
            />
          </label>
          <button
            type="submit"
            className="platform-page-primary-button w-full"
            disabled={formState.status === "submitting"}
          >
            提交重置请求
          </button>
        </form>
        <Link
          to="/auth/login"
          className="mt-5 inline-flex text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]"
        >
          返回登录
        </Link>
      </section>
    </PlatformPageShell>
  );
}

export default ForgotPasswordPage;
