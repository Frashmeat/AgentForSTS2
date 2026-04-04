import { Link, useNavigate } from "react-router-dom";
import { logoutSession } from "../shared/api/auth.ts";
import { useSession } from "../shared/session/hooks.ts";

function renderAvatar(username: string) {
  return username.trim().slice(0, 1).toUpperCase() || "U";
}

export function UserEntry() {
  const navigate = useNavigate();
  const { currentUser, isAuthenticated, isLoading, markSignedOut } = useSession();

  async function handleLogout() {
    try {
      await logoutSession();
    } finally {
      markSignedOut();
      navigate("/auth/login", { replace: true });
    }
  }

  if (isLoading) {
    return <span className="text-sm text-slate-400">会话恢复中...</span>;
  }

  if (!isAuthenticated || currentUser === null) {
    return (
      <div className="flex items-center gap-2">
        <Link
          to="/auth/login"
          className="rounded-lg border border-slate-200 px-3 py-1.5 text-sm font-medium text-slate-600 transition hover:border-slate-300 hover:text-slate-900"
        >
          登录
        </Link>
        <Link
          to="/auth/register"
          className="rounded-lg bg-amber-500 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-amber-600"
        >
          注册
        </Link>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-3">
      <Link to="/me" className="flex items-center gap-2 rounded-full border border-slate-200 px-2 py-1 pr-3 transition hover:border-amber-300">
        <span className="flex h-8 w-8 items-center justify-center rounded-full bg-amber-100 text-sm font-semibold text-amber-700">
          {renderAvatar(currentUser.username)}
        </span>
        <span className="text-sm font-medium text-slate-700">{currentUser.username}</span>
      </Link>
      <button
        type="button"
        className="text-sm text-slate-500 transition hover:text-slate-700"
        onClick={() => {
          void handleLogout();
        }}
      >
        退出
      </button>
    </div>
  );
}

export default UserEntry;
