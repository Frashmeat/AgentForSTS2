import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { requestPasswordReset } from "../../shared/api/auth.ts";
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
    <section className="mx-auto w-full max-w-md rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">找回密码</h1>
      <p className="mt-2 text-sm text-slate-500">输入登录名或邮箱，获取重置密码所需的验证码。</p>
      <form className="mt-6 space-y-4" onSubmit={handleSubmit}>
        <label className="block text-sm font-medium text-slate-700">
          登录名
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
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
          className="w-full rounded-xl bg-amber-500 px-4 py-2 font-medium text-white transition hover:bg-amber-600"
          disabled={formState.status === "submitting"}
        >
          获取重置码
        </button>
      </form>
      <Link to="/auth/login" className="mt-4 inline-flex text-sm text-amber-600 hover:text-amber-700">
        返回登录
      </Link>
    </section>
  );
}

export default ForgotPasswordPage;
