export const DEFAULT_ERROR_MESSAGE = "请求失败，请稍后重试";

type StructuredErrorPayload = {
  error?: unknown;
  message?: unknown;
  detail?: unknown;
  traceback?: unknown;
};

function pickText(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value !== "string") {
      continue;
    }
    const trimmed = value.trim();
    if (trimmed) {
      return trimmed;
    }
  }
  return null;
}

function maybeParseJsonString(value: string): unknown | null {
  const trimmed = value.trim();
  if (!trimmed || !["{", "[", "\""].includes(trimmed[0] ?? "")) {
    return null;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    return null;
  }
}

function readStructuredErrorMessage(payload: StructuredErrorPayload): string | null {
  if (typeof payload.error === "object" && payload.error !== null) {
    const nested = payload.error as StructuredErrorPayload;
    const nestedMessage = pickText(nested.message, nested.detail);
    if (nestedMessage) {
      return nestedMessage;
    }
  }

  if (typeof payload.error === "string") {
    return pickText(payload.error);
  }

  return pickText(payload.message, payload.detail);
}

export function resolveErrorMessage(error: unknown, fallback = DEFAULT_ERROR_MESSAGE): string {
  if (error instanceof Error) {
    return resolveErrorMessage(error.message, fallback);
  }

  if (typeof error === "string") {
    const parsed = maybeParseJsonString(error);
    if (parsed !== null) {
      return resolveErrorMessage(parsed, fallback);
    }
    return error.trim() || fallback;
  }

  if (typeof error === "number" || typeof error === "boolean" || typeof error === "bigint") {
    return String(error);
  }

  if (typeof error === "object" && error !== null) {
    const message = readStructuredErrorMessage(error as StructuredErrorPayload);
    if (message) {
      return message;
    }
  }

  return fallback;
}

export function resolveWorkflowErrorMessage(error: unknown, fallback = DEFAULT_ERROR_MESSAGE): string {
  return resolveErrorMessage(error, fallback);
}
