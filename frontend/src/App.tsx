import { useState } from "react";
import { House } from "lucide-react";
import { Link, Navigate, Route, Routes, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import ExecutionModeDialog from "./components/ExecutionModeDialog.tsx";
import { KnowledgeGuideDialog } from "./components/KnowledgeGuideDialog.tsx";
import { PlatformAuthUnavailableNotice } from "./components/PlatformAuthUnavailableNotice.tsx";
import { PlatformPageShell } from "./components/platform/PlatformPageShell.tsx";
import { WorkspaceShell } from "./components/workspace/WorkspaceShell.tsx";
import { ForgotPasswordPage } from "./features/auth/ForgotPasswordPage.tsx";
import { LoginPage } from "./features/auth/LoginPage.tsx";
import { RegisterPage } from "./features/auth/RegisterPage.tsx";
import { ResetPasswordPage } from "./features/auth/ResetPasswordPage.tsx";
import { VerifyEmailPage } from "./features/auth/VerifyEmailPage.tsx";
import { UserCenterJobDetailPage } from "./features/user-center/job-detail-page.tsx";
import { UserCenterPage } from "./features/user-center/page.tsx";
import type { WorkspaceTab } from "./features/platform-run/types.ts";
import {
  buildSettingsPath,
  buildWorkspacePath,
  resolveWorkspaceTab,
  workspaceNavItems,
} from "./features/workspace/config.ts";
import { WorkspaceProvider } from "./features/workspace/WorkspaceContext.tsx";
import { useExecutionModeFlow } from "./features/workspace/useExecutionModeFlow.ts";
import { useKnowledgeStatusFlow } from "./features/workspace/useKnowledgeStatusFlow.ts";
import { WorkspaceHome } from "./features/workspace/WorkspaceHome.tsx";
import { useSession } from "./shared/session/hooks.ts";
import { SettingsPage } from "./pages/SettingsPage.tsx";

type AppTab = WorkspaceTab;

function buildPlatformAuthUnavailableElement(title: string, description: string) {
  return (
    <PlatformPageShell
      kicker="Platform Access"
      title={title}
      description="当前环境还没有接入独立 Web 平台服务，因此认证与用户中心能力不可用。"
      actions={
        <Link to="/" className="platform-page-action-link">
          <House size={16} />
          <span>返回首页</span>
        </Link>
      }
      width="narrow"
    >
      <PlatformAuthUnavailableNotice title={title} description={description} />
    </PlatformPageShell>
  );
}

export default function App() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { isAuthAvailable, isAuthenticated } = useSession();
  const activeTab: AppTab = resolveWorkspaceTab(searchParams.get("tab"));
  const [knowledgeGuideOpen, setKnowledgeGuideOpen] = useState(false);
  const {
    pendingExecution,
    serverProfiles,
    serverProfilesLoading,
    serverProfilesError,
    selectedServerProfileId,
    rememberServerProfile,
    handleExecutionRequest,
    handleChooseLocalExecution,
    handleChooseServerExecution,
    handleGoLoginForServerExecution,
    closeExecutionDialog,
    handleReloadServerProfiles,
    setSelectedServerProfileId,
    setRememberServerProfile,
  } = useExecutionModeFlow({
    isAuthenticated,
  });
  const {
    knowledgeStatus,
    handleRefreshKnowledge,
  } = useKnowledgeStatusFlow();

  function updateActiveTab(nextTab: AppTab) {
    if (location.pathname !== "/") {
      return;
    }
    const nextSearchParams = new URLSearchParams(searchParams);
    if (nextTab === "single") {
      nextSearchParams.delete("tab");
    } else {
      nextSearchParams.set("tab", nextTab);
    }
    setSearchParams(nextSearchParams, { replace: true });
  }

  return (
    <div className="min-h-screen bg-[var(--workspace-bg)] text-slate-800">
      <Routes>
        <Route
          path="/"
          element={
            <WorkspaceShell
              activeTab={activeTab}
              navItems={workspaceNavItems}
              onTabChange={updateActiveTab}
              onOpenSettings={() => navigate(buildSettingsPath(buildWorkspacePath(activeTab)))}
            >
              <WorkspaceProvider
                value={{
                  knowledgeStatus,
                  onRequestExecution: handleExecutionRequest,
                  onRefreshKnowledge: () => {
                    void handleRefreshKnowledge();
                  },
                  onOpenKnowledgeGuide: () => setKnowledgeGuideOpen(true),
                  onOpenSettings: () => navigate(buildSettingsPath(buildWorkspacePath(activeTab))),
                }}
              >
                <WorkspaceHome activeTab={activeTab} />
              </WorkspaceProvider>
            </WorkspaceShell>
          }
        />
        <Route path="/settings" element={<SettingsPage />} />
        <Route
          path="/auth/login"
          element={isAuthAvailable ? <LoginPage /> : buildPlatformAuthUnavailableElement("当前环境不支持登录", "这是本机工作站模式，未接入独立 Web 平台服务，因此登录与注册入口不可用。")}
        />
        <Route
          path="/auth/register"
          element={isAuthAvailable ? <RegisterPage /> : buildPlatformAuthUnavailableElement("当前环境不支持注册", "这是本机工作站模式，未接入独立 Web 平台服务，因此无法创建平台账号。")}
        />
        <Route
          path="/auth/verify-email"
          element={isAuthAvailable ? <VerifyEmailPage /> : buildPlatformAuthUnavailableElement("当前环境不支持邮箱验证", "请先接入独立 Web 平台服务，再使用平台账号验证链路。")}
        />
        <Route
          path="/auth/forgot-password"
          element={isAuthAvailable ? <ForgotPasswordPage /> : buildPlatformAuthUnavailableElement("当前环境不支持密码找回", "请先接入独立 Web 平台服务，再使用平台账号密码找回功能。")}
        />
        <Route
          path="/auth/reset-password"
          element={isAuthAvailable ? <ResetPasswordPage /> : buildPlatformAuthUnavailableElement("当前环境不支持密码重置", "请先接入独立 Web 平台服务，再使用平台账号密码重置功能。")}
        />
        <Route
          path="/me"
          element={isAuthAvailable ? <UserCenterPage /> : buildPlatformAuthUnavailableElement("当前环境未启用用户中心", "这是本机工作站模式。只有接入独立 Web 平台服务后，用户中心、任务记录和次数池才可用。")}
        />
        <Route
          path="/me/jobs/:jobId"
          element={isAuthAvailable ? <UserCenterJobDetailPage /> : buildPlatformAuthUnavailableElement("当前环境未启用用户中心", "这是本机工作站模式。只有接入独立 Web 平台服务后，平台任务详情才可用。")}
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <KnowledgeGuideDialog open={knowledgeGuideOpen} status={knowledgeStatus} onClose={() => setKnowledgeGuideOpen(false)} />
      <ExecutionModeDialog
        open={pendingExecution !== null}
        title={pendingExecution?.title ?? "选择执行方式"}
        localAvailable={pendingExecution?.localAvailable ?? false}
        localUnavailableReasons={pendingExecution?.localUnavailableReasons ?? []}
        isAuthenticated={isAuthenticated}
        serverProfiles={serverProfiles}
        serverProfilesLoading={serverProfilesLoading}
        serverProfilesError={serverProfilesError}
        selectedServerProfileId={selectedServerProfileId}
        rememberServerProfile={rememberServerProfile}
        onClose={closeExecutionDialog}
        onChooseLocal={handleChooseLocalExecution}
        onChooseServer={() => {
          void handleChooseServerExecution();
        }}
        onGoLogin={handleGoLoginForServerExecution}
        onSelectServerProfile={(profileId) => setSelectedServerProfileId(profileId)}
        onRememberServerProfileChange={(value) => setRememberServerProfile(value)}
        onReloadServerProfiles={() => {
          void handleReloadServerProfiles();
        }}
      />
    </div>
  );
}
