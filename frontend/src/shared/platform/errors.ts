export type PlatformErrorOrigin = "web" | "web_workstation" | "local_workstation" | "upstream" | "unknown";
export type PlatformRuntimeSurface = "web" | "web_workstation" | "local_workstation" | "browser" | "unknown";

export interface PlatformErrorView {
  schemaVersion: string;
  title: string;
  badgeLabel: string;
  badgeTone: "danger" | "warning" | "info";
  message: string;
  developerMessage: string;
  origin: PlatformErrorOrigin;
  runtimeSurface: PlatformRuntimeSurface;
  component: string;
  operation: string;
  category: string;
  reasonCode: string;
  retryable: boolean | null;
  stepId: string;
  stepType: string;
  apiProtocol: string;
  model: string;
  httpStatus: number | null;
  logHint: {
    primary: string;
    secondary: string;
  };
  diagnosticRows: Array<{ label: string; value: string }>;
}

type ErrorPayload = Record<string, unknown>;

const DIAGNOSTIC_LABELS: Record<string, string> = {
  content_type: "Content-Type",
  body_tail: "响应尾部",
  raw_error: "上游原始摘要",
  upstream_category: "上游分类",
  exception_type: "异常类型",
  traceback: "Traceback",
  stdout_tail: "stdout 尾部",
  stderr_tail: "stderr 尾部",
  initial_error: "初始上游错误",
  final_error: "最终上游错误",
  endpoint: "请求端点",
  upstream_attempts: "上游尝试链",
};

export function toPlatformErrorView(errorPayload: unknown, errorSummary = ""): PlatformErrorView {
  const payload = isRecord(errorPayload) ? errorPayload : {};
  const schemaVersion = readString(payload.schema_version);
  const origin = parseOrigin(payload.origin);
  const runtimeSurface = parseRuntimeSurface(payload.runtime_surface);
  const reasonCode = readString(payload.reason_code);
  const message = readString(payload.message) || errorSummary || "任务执行失败";

  return {
    schemaVersion,
    title: titleForOrigin(origin),
    badgeLabel: labelForOrigin(origin),
    badgeTone: toneForOrigin(origin),
    message,
    developerMessage: readString(payload.developer_message),
    origin,
    runtimeSurface,
    component: readString(payload.component),
    operation: readString(payload.operation),
    category: readString(payload.category),
    reasonCode,
    retryable: typeof payload.retryable === "boolean" ? payload.retryable : null,
    stepId: readString(payload.step_id),
    stepType: readString(payload.step_type),
    apiProtocol: readString(payload.api_protocol),
    model: readString(payload.model),
    httpStatus: readNumber(payload.http_status),
    logHint: readLogHint(payload.log_hint),
    diagnosticRows: readDiagnosticRows(payload.diagnostic),
  };
}

export function hasPlatformErrorPayload(errorPayload: unknown): boolean {
  return isRecord(errorPayload) && readString(errorPayload.schema_version) === "platform_error.v1";
}

function titleForOrigin(origin: PlatformErrorOrigin): string {
  switch (origin) {
    case "upstream":
      return "上游模型错误";
    case "web":
      return "Web 服务错误";
    case "web_workstation":
      return "Web 工作站错误";
    case "local_workstation":
      return "本机工作站错误";
    default:
      return "任务执行失败";
  }
}

function labelForOrigin(origin: PlatformErrorOrigin): string {
  return titleForOrigin(origin);
}

function toneForOrigin(origin: PlatformErrorOrigin): "danger" | "warning" | "info" {
  if (origin === "upstream" || origin === "local_workstation") {
    return "warning";
  }
  if (origin === "unknown") {
    return "info";
  }
  return "danger";
}

function parseOrigin(value: unknown): PlatformErrorOrigin {
  const text = readString(value);
  if (text === "web" || text === "web_workstation" || text === "local_workstation" || text === "upstream") {
    return text;
  }
  return "unknown";
}

function parseRuntimeSurface(value: unknown): PlatformRuntimeSurface {
  const text = readString(value);
  if (text === "web" || text === "web_workstation" || text === "local_workstation" || text === "browser") {
    return text;
  }
  return "unknown";
}

function readLogHint(value: unknown): { primary: string; secondary: string } {
  if (!isRecord(value)) {
    return { primary: "", secondary: "" };
  }
  return {
    primary: readString(value.primary),
    secondary: readString(value.secondary),
  };
}

function readDiagnosticRows(value: unknown): Array<{ label: string; value: string }> {
  if (!isRecord(value)) {
    return [];
  }
  return Object.entries(DIAGNOSTIC_LABELS)
    .map(([key, label]) => ({ label, value: readString(value[key]) }))
    .filter((row) => row.value.length > 0);
}

function readString(value: unknown): string {
  if (typeof value === "string") {
    return value.trim();
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value) || isRecord(value)) {
    return JSON.stringify(value);
  }
  return "";
}

function readNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function isRecord(value: unknown): value is ErrorPayload {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
