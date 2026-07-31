export type FailureCategory =
  | "configuration"
  | "authentication"
  | "rate_limit"
  | "network"
  | "upstream"
  | "validation"
  | "filesystem"
  | "toolchain"
  | "state"
  | "internal";

export type RecoveryAction =
  | "configure"
  | "reauthenticate"
  | "retry"
  | "check_path"
  | "install_dependency"
  | "open_settings"
  | "none";

export type FailureIoKind =
  | "not_found"
  | "permission_denied"
  | "already_exists"
  | "invalid_input"
  | "timed_out"
  | "connection_refused"
  | "connection_reset"
  | "broken_pipe"
  | "unexpected_eof"
  | "other";

export interface FailureContext {
  provider?: string;
  httpStatus?: number;
  runId?: string;
  projectRelativePath?: string;
  settingKey?: string;
  dependency?: string;
}

export interface FailureDiagnostic {
  id: string;
  summary: string;
  ioKind?: FailureIoKind;
}

export interface ActionableFailure {
  schemaVersion: 1;
  code: string;
  category: FailureCategory;
  stage: string;
  message: string;
  action: RecoveryAction;
  retryable: boolean;
  retryAfterMs?: number;
  context?: FailureContext;
  diagnostic?: FailureDiagnostic;
}

const CATEGORIES = new Set<FailureCategory>([
  "configuration",
  "authentication",
  "rate_limit",
  "network",
  "upstream",
  "validation",
  "filesystem",
  "toolchain",
  "state",
  "internal",
]);

const ACTIONS = new Set<RecoveryAction>([
  "configure",
  "reauthenticate",
  "retry",
  "check_path",
  "install_dependency",
  "open_settings",
  "none",
]);

const IO_KINDS = new Set<FailureIoKind>([
  "not_found",
  "permission_denied",
  "already_exists",
  "invalid_input",
  "timed_out",
  "connection_refused",
  "connection_reset",
  "broken_pipe",
  "unexpected_eof",
  "other",
]);

const FAILURE_KEYS = new Set([
  "schemaVersion",
  "code",
  "category",
  "stage",
  "message",
  "action",
  "retryable",
  "retryAfterMs",
  "context",
  "diagnostic",
]);
const CONTEXT_KEYS = new Set([
  "provider",
  "httpStatus",
  "runId",
  "projectRelativePath",
  "settingKey",
  "dependency",
]);
const DIAGNOSTIC_KEYS = new Set(["id", "summary", "ioKind"]);
const SAFE_CODE = /^[a-z0-9._]+$/;
const SAFE_SEGMENT = /^[A-Za-z0-9_-]+$/;
const SAFE_CONTEXT_VALUE = /^[A-Za-z0-9._:-]+$/;

let fallbackCounter = 0;

export function isActionableFailure(value: unknown): value is ActionableFailure {
  if (!isRecord(value)) return false;
  if (
    !hasOnlyKeys(value, FAILURE_KEYS) ||
    value.schemaVersion !== 1 ||
    !isBoundedString(value.code, 256) ||
    !SAFE_CODE.test(value.code) ||
    !isBoundedString(value.stage, 128) ||
    !isBoundedString(value.message, 2048) ||
    !CATEGORIES.has(value.category as FailureCategory) ||
    !ACTIONS.has(value.action as RecoveryAction) ||
    typeof value.retryable !== "boolean"
  ) {
    return false;
  }
  if (
    value.retryAfterMs !== undefined &&
    (!Number.isSafeInteger(value.retryAfterMs) ||
      (value.retryAfterMs as number) < 0 ||
      (value.retryAfterMs as number) > 86_400_000)
  ) {
    return false;
  }
  return isFailureContext(value.context) && isFailureDiagnostic(value.diagnostic);
}

export function toActionableFailure(value: unknown): ActionableFailure {
  return isActionableFailure(value) ? value : unclassifiedClientFailure();
}

export function localValidationFailure(
  stage: string,
  message: string,
): ActionableFailure {
  return {
    schemaVersion: 1,
    code: "run.input_invalid",
    category: "validation",
    stage: stage.slice(0, 128) || "client.validation",
    message: message.slice(0, 2048) || "Check the input and try again.",
    action: "none",
    retryable: false,
  };
}

function unclassifiedClientFailure(): ActionableFailure {
  fallbackCounter += 1;
  return {
    schemaVersion: 1,
    code: "core.unclassified",
    category: "internal",
    stage: "client.invoke",
    message: "An unexpected error occurred. Try again or review the diagnostic ID.",
    action: "retry",
    retryable: true,
    diagnostic: {
      id: `diag-client-${fallbackCounter}`,
      summary: "The command returned an invalid failure payload; raw data was not exposed.",
    },
  };
}

function isFailureContext(value: unknown): value is FailureContext | undefined {
  if (value === undefined) return true;
  if (!isRecord(value)) return false;
  return (
    hasOnlyKeys(value, CONTEXT_KEYS) &&
    isOptionalSafeContextValue(value.provider) &&
    (value.httpStatus === undefined ||
      (Number.isInteger(value.httpStatus) &&
        (value.httpStatus as number) >= 100 &&
        (value.httpStatus as number) <= 599)) &&
    isOptionalSafeSegment(value.runId) &&
    (value.projectRelativePath === undefined ||
      isSafeRelativePath(value.projectRelativePath)) &&
    isOptionalSafeContextValue(value.settingKey) &&
    isOptionalSafeContextValue(value.dependency)
  );
}

function isFailureDiagnostic(
  value: unknown,
): value is FailureDiagnostic | undefined {
  if (value === undefined) return true;
  if (!isRecord(value)) return false;
  return (
    hasOnlyKeys(value, DIAGNOSTIC_KEYS) &&
    isBoundedString(value.id, 256) &&
    SAFE_SEGMENT.test(value.id) &&
    isBoundedString(value.summary, 512) &&
    (value.ioKind === undefined || IO_KINDS.has(value.ioKind as FailureIoKind))
  );
}

function isOptionalSafeContextValue(value: unknown): boolean {
  return (
    value === undefined ||
    (isBoundedString(value, 256) && SAFE_CONTEXT_VALUE.test(value))
  );
}

function isOptionalSafeSegment(value: unknown): boolean {
  return value === undefined || (isBoundedString(value, 256) && SAFE_SEGMENT.test(value));
}

function isSafeRelativePath(value: unknown): boolean {
  return (
    isBoundedString(value, 256) &&
    !value.includes("\\") &&
    !value.includes(":") &&
    !value.startsWith("/") &&
    value.split("/").every((part) => part.length > 0 && part !== "." && part !== "..")
  );
}

function isBoundedString(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: Set<string>): boolean {
  return Object.keys(value).every((key) => allowed.has(key));
}
