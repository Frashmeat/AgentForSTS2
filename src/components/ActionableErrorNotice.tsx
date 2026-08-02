import type { ActionableFailure, RecoveryAction } from "../services/actionableFailure";
import { Notice } from "./ui/Notice";

interface ActionableErrorNoticeProps {
  failure: ActionableFailure | null;
  className?: string;
}

const ACTION_LABELS: Record<RecoveryAction, string | null> = {
  check_settings: "检查设置后重试",
  open_project: "打开工程后重试",
  refresh_truth: "刷新 Truth 后重试",
  replace_resource: "替换资源后重试",
  inspect_run: "检查 Run 记录",
  configure: "检查配置后重试",
  reauthenticate: "更新凭据后重试",
  retry: "请重试",
  check_path: "检查文件路径",
  install_dependency: "安装或修复依赖",
  open_settings: "打开设置检查配置",
  none: null,
};

export function ActionableErrorNotice({
  failure,
  className,
}: ActionableErrorNoticeProps) {
  if (!failure) return null;
  const action = ACTION_LABELS[failure.action];
  return (
    <Notice variant="error" title="操作失败" className={className}>
      <p>{failure.message}</p>
      {action && <p>{action}</p>}
      {failure.diagnostic && (
        <p>
          诊断 ID：<code>{failure.diagnostic.id}</code>
        </p>
      )}
    </Notice>
  );
}
