import { Link, useNavigate } from "react-router-dom";
import { logoutSession } from "../shared/api/auth.ts";
import { useSession } from "../shared/session/hooks.ts";

function renderAvatar(username: string) {
  return username.trim().slice(0, 1).toUpperCase() || "U";
}

export function UserEntry() {
  const navigate = useNavigate();
  const { currentUser, isAuthAvailable, isAuthenticated, isLoading, markSignedOut } = useSession();

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

  if (!isAuthAvailable) {
    return <span className="text-sm text-slate-400">平台账号未启用</span>;
  }

  if (!isAuthenticated || currentUser === null) {
    return (
      <div className="flex items-center gap-2">
        <Link
          to="/auth/login"
          className="rounded-xl border border-[var(--workspace-panel-border)] bg-white/70 px-3 py-1.5 text-sm font-medium text-[var(--workspace-accent-strong)] transition hover:border-[rgba(114,74,145,0.35)] hover:text-[var(--workspace-accent)]"
        >
          登录
        </Link>
        <Link
          to="/auth/register"
          className="rounded-xl bg-[linear-gradient(135deg,var(--workspace-accent-strong),var(--workspace-accent))] px-3 py-1.5 text-sm font-medium text-white transition hover:brightness-110"
        >
          注册
        </Link>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-3">
      <Link
        to="/admin"
        className="rounded-xl border border-[var(--workspace-panel-border)] bg-white/70 px-3 py-1.5 text-sm font-medium text-[var(--workspace-accent-strong)] transition hover:border-[rgba(114,74,145,0.35)] hover:text-[var(--workspace-accent)]"
      >
        管理台
      </Link>
      <Link to="/me" className="flex items-center gap-2 rounded-full border border-[var(--workspace-panel-border)] bg-white/75 px-2 py-1 pr-3 transition hover:border-[rgba(114,74,145,0.35)]">
        <span className="flex h-8 w-8 items-center justify-center rounded-full bg-[rgba(114,74,145,0.12)] text-sm font-semibold text-[var(--workspace-accent-strong)]">
          {renderAvatar(currentUser.username)}
        </span>
        <span className="text-sm font-medium text-slate-700">{currentUser.username}</span>
      </Link>
      <button
        type="button"
        className="text-sm text-[var(--workspace-text-muted)] transition hover:text-[var(--workspace-accent-strong)]"
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
