import { useEffect, useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { resendVerification, verifyEmailCode } from "../../shared/api/auth.ts";
import {
  createErrorAuthFormState,
  createIdleAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "./formModel.ts";

export function VerifyEmailPage() {
  const [searchParams] = useSearchParams();
  const [code, setCode] = useState(() => searchParams.get("code") ?? "");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [formState, setFormState] = useState(createIdleAuthFormState);

  useEffect(() => {
    setCode(searchParams.get("code") ?? "");
  }, [searchParams]);

  async function handleVerify(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormState(createSubmittingAuthFormState());
    try {
      await verifyEmailCode(code);
      setFormState(createSuccessAuthFormState("邮箱验证成功，可以直接登录。"));
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "邮箱验证失败"));
    }
  }

  async function handleResend() {
    if (!login.trim() || !password.trim()) {
      setFormState(createErrorAuthFormState("重发验证码需要输入登录名和密码"));
      return;
    }
    setFormState(createSubmittingAuthFormState());
    try {
      const response = await resendVerification({ login, password });
      setCode(response.verification_code);
      setFormState(createSuccessAuthFormState(`新的验证码：${response.verification_code}`));
    } catch (error) {
      setFormState(createErrorAuthFormState(error instanceof Error ? error.message : "重发失败"));
    }
  }

  return (
    <section className="mx-auto w-full max-w-md rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">验证邮箱</h1>
      <p className="mt-2 text-sm text-slate-500">注册后先完成邮箱验证，再开始平台模式。</p>
      <form className="mt-6 space-y-4" onSubmit={handleVerify}>
        <label className="block text-sm font-medium text-slate-700">
          验证码
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={code}
            onChange={event => setCode(event.target.value)}
          />
        </label>
        <label className="block text-sm font-medium text-slate-700">
          重发时使用的登录名
          <input
            className="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 outline-none focus:border-amber-500"
            value={login}
            onChange={event => setLogin(event.target.value)}
          />
        </label>
        <label className="block text-sm font-medium text-slate-700">
          重发时使用的密码
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
        <div className="flex gap-3">
          <button
            type="submit"
            className="flex-1 rounded-xl bg-amber-500 px-4 py-2 font-medium text-white transition hover:bg-amber-600"
            disabled={formState.status === "submitting"}
          >
            验证邮箱
          </button>
          <button
            type="button"
            className="rounded-xl border border-slate-200 px-4 py-2 font-medium text-slate-700 transition hover:border-slate-300"
            onClick={() => {
              void handleResend();
            }}
          >
            重发验证码
          </button>
        </div>
      </form>
      <Link to="/auth/login" className="mt-4 inline-flex text-sm text-amber-600 hover:text-amber-700">
        返回登录
      </Link>
    </section>
  );
}

export default VerifyEmailPage;
