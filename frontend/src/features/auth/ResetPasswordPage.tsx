import { useState, type FormEvent } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { resetPasswordWithCode } from "../../shared/api/auth.ts";
import { useSession } from "../../shared/session/hooks.ts";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
} from "./formModel.ts";

export function ResetPasswordPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { markSignedIn } = useSession();
  const [code, setCode] = useState(() => searchParams.get("code") ?? "");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      const response = await resetPasswordWithCode(code, password);
      markSignedIn(response.user);
      navigate("/", { replace: true });
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "重置密码失败"));
    }
  }

  return (
    <section className="mx-auto w-full max-w-md rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">重置密码</h1>
      <p className="mt-2 text-sm text-slate-500">输入收到的重置码，设置新的登录密码。</p>
      <form className="mt-6 space-y-4" onSubmit={handleSubmit}>
        <label className="block text-sm font-medium text-slate-700">
          重置码
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={code}
            onChange={event => setCode(event.target.value)}
          />
        </label>
        <label className="block text-sm font-medium text-slate-700">
          新密码
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
          {formState.status === "submitting" ? "提交中..." : "重置密码"}
        </button>
      </form>
      <Link to="/auth/login" className="mt-4 inline-flex text-sm text-amber-600 hover:text-amber-700">
        返回登录
      </Link>
    </section>
  );
}

export default ResetPasswordPage;
