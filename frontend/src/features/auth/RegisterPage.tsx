import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { registerWithPassword } from "../../shared/api/auth.ts";
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
    <section className="mx-auto w-full max-w-md rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">注册</h1>
      <p className="mt-2 text-sm text-slate-500">创建平台账号后，可通过用户中心查看服务器任务记录。</p>
      <form className="mt-6 space-y-4" onSubmit={handleSubmit}>
        <label className="block text-sm font-medium text-slate-700">
          用户名
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={username}
            onChange={event => setUsername(event.target.value)}
          />
        </label>
        <label className="block text-sm font-medium text-slate-700">
          邮箱
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={email}
            onChange={event => setEmail(event.target.value)}
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
          {formState.status === "submitting" ? "提交中..." : "注册"}
        </button>
      </form>
      <p className="mt-4 text-sm text-slate-500">
        已有账号？
        <Link to="/auth/login" className="ml-1 text-amber-600 hover:text-amber-700">
          去登录
        </Link>
      </p>
    </section>
  );
}

export default RegisterPage;
