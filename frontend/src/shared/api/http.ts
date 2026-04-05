export type BackendTarget = "same-origin" | "workstation" | "web";

export interface RequestJsonOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
  backend?: BackendTarget;
}

export function buildApiPath(path: string, query: Record<string, string | number | undefined>): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (typeof value === "undefined") {
      continue;
    }
    params.set(key, String(value));
  }
  const queryString = params.toString();
  return queryString ? `${path}?${queryString}` : path;
}

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

function inferBackendBaseFromLocation(target: BackendTarget): string {
  if (target !== "web") {
    return "";
  }

  const locationLike = (globalThis as typeof globalThis & {
    location?: Location | URL;
  }).location;

  if (typeof locationLike === "undefined") {
    return "";
  }

  try {
    const currentUrl = new URL(locationLike.href);
    if (currentUrl.port === "7870") {
      return trimTrailingSlash(currentUrl.origin);
    }

    currentUrl.port = "7870";
    return trimTrailingSlash(currentUrl.origin);
  } catch {
    return "";
  }
}

function readRuntimeApiBase(target: BackendTarget): string {
  const runtimeBases = (
    globalThis as typeof globalThis & {
      __AGENT_THE_SPIRE_API_BASES__?: Partial<Record<Exclude<BackendTarget, "same-origin">, string>>;
    }
  ).__AGENT_THE_SPIRE_API_BASES__;

  if (target === "same-origin") {
    return "";
  }

  const configured = runtimeBases?.[target];
  return typeof configured === "string" ? trimTrailingSlash(configured) : "";
}

export function resolveBackendBaseUrl(target: BackendTarget): string {
  if (target === "same-origin") {
    return "";
  }
  const configuredBase = readRuntimeApiBase(target);
  if (configuredBase) {
    return configuredBase;
  }
  return inferBackendBaseFromLocation(target);
}

export function buildBackendUrl(path: string, target: BackendTarget): string {
  const baseUrl = resolveBackendBaseUrl(target);
  if (!baseUrl) {
    return path;
  }
  if (/^https?:\/\//.test(path)) {
    return path;
  }
  return path.startsWith("/") ? `${baseUrl}${path}` : `${baseUrl}/${path}`;
}

export async function requestJson<T>(path: string, options: RequestJsonOptions = {}): Promise<T> {
  const {
    body,
    headers,
    method = "GET",
    backend = "same-origin",
    credentials,
    ...rest
  } = options;
  const hasBody = typeof body !== "undefined";
  const response = await fetch(buildBackendUrl(path, backend), {
    ...rest,
    method,
    credentials: credentials ?? (backend === "same-origin" ? undefined : "include"),
    headers: hasBody
      ? {
          "Content-Type": "application/json",
          ...(headers ?? {}),
        }
      : headers,
    body: hasBody ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    throw new Error(await response.text());
  }

  return response.json();
}
