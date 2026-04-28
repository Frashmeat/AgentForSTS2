export type AdminDisplayTone = "neutral" | "success" | "warning" | "danger" | "info";

export type AdminDisplayLabel = {
  label: string;
  tone: AdminDisplayTone;
};

const statusLabels: Record<string, AdminDisplayLabel> = {
  healthy: { label: "健康", tone: "success" },
  degraded: { label: "需复检", tone: "warning" },
  disabled: { label: "已停用", tone: "neutral" },
  auth_failed: { label: "认证失败", tone: "danger" },
  rate_limited: { label: "调用限流", tone: "warning" },
  quota_exhausted: { label: "额度耗尽", tone: "danger" },
  succeeded: { label: "已成功", tone: "success" },
  success: { label: "已成功", tone: "success" },
  failed: { label: "已失败", tone: "danger" },
  retrying: { label: "重试中", tone: "warning" },
  running: { label: "执行中", tone: "info" },
  pending: { label: "等待中", tone: "warning" },
  charged: { label: "已扣减", tone: "success" },
  refunded: { label: "已返还", tone: "success" },
};

const eventTypeLabels: Record<string, string> = {
  "runtime.queue_worker.leader_acquired": "取得调度权",
  "runtime.queue_worker.leader_renewed": "调度权续租",
  "runtime.queue_worker.leader_taken_over": "接管调度权",
  "runtime.queue_worker.leader_observed_other": "观察到其他调度者",
  "runtime.queue_worker.leader_lost": "失去调度权",
  "runtime.queue_worker.leader_released": "释放调度权",
  "runtime.queue_worker.leader_waiting_for_failover": "等待故障转移",
  "runtime.queue_worker.job_claimed": "领取任务",
  "runtime.queue_worker.workspace_locked": "工作区占用",
  leader_acquired: "取得调度权",
  leader_taken_over: "接管调度权",
  leader_observed_other: "观察到其他调度者",
  leader_lost: "失去调度权",
  leader_released: "释放调度权",
  leader_waiting_for_failover: "等待故障转移",
  "quota.refunded": "额度返还",
};

const authTypeLabels: Record<string, string> = {
  api_key: "密钥",
  ak_sk: "访问密钥",
};

const providerLabels: Record<string, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
};

const refundReasonLabels: Record<string, string> = {
  execution_failed: "执行失败",
  credential_failed: "凭据失败",
  timeout: "执行超时",
  execution_timeout: "执行超时",
};

export function formatAdminStatus(status?: string | null): AdminDisplayLabel {
  const key = String(status ?? "").trim();
  if (!key) {
    return { label: "未知", tone: "neutral" };
  }
  return statusLabels[key] ?? { label: key, tone: "neutral" };
}

export function formatAdminEventType(eventType?: string | null): string {
  const key = String(eventType ?? "").trim();
  if (!key) {
    return "未知事件";
  }
  return eventTypeLabels[key] ?? key;
}

export function formatAdminAuthType(authType?: string | null): string {
  const key = String(authType ?? "").trim();
  if (!key) {
    return "未知认证";
  }
  return authTypeLabels[key] ?? key;
}

export function formatAdminProvider(provider?: string | null): string {
  const key = String(provider ?? "").trim();
  if (!key) {
    return "未知服务商";
  }
  return providerLabels[key] ?? key;
}

export function formatAdminRefundReason(reason?: string | null): string {
  const key = String(reason ?? "").trim();
  if (!key) {
    return "未记录";
  }
  return refundReasonLabels[key] ?? key;
}
