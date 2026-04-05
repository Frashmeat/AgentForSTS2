export type BackendTarget = "same-origin" | "workstation" | "web";
export type WebSocketTarget = "workstation";

export interface RequestJsonOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
  backend?: BackendTarget;
}

type RuntimeApiBases = Partial<Record<Exclude<BackendTarget, "same-origin">, string>>;
type RuntimeWsBases = Partial<Record<WebSocketTarget, string>>;

const IMPLICIT_WORKSTATION_PORTS = new Set(["5173", "7860"]);
const DEFAULT_FRONTEND_HTTP_PROTOCOL = "http:";
const DEFAULT_FRONTEND_WS_PROTOCOL = "ws:";

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

function readLocationUrl(): URL | null {
  const locationLike = (globalThis as typeof globalThis & {
    location?: Partial<Location> | URL;
  }).location;

  if (typeof locationLike === "undefined") {
    return null;
  }

  if (typeof locationLike.href === "string" && locationLike.href.length > 0) {
    try {
      return new URL(locationLike.href);
    } catch {
      return null;
    }
  }

  if (typeof locationLike.host === "string" && locationLike.host.length > 0) {
    const protocol = typeof locationLike.protocol === "string" ? locationLike.protocol : DEFAULT_FRONTEND_HTTP_PROTOCOL;
    const pathname = typeof locationLike.pathname === "string" && locationLike.pathname.length > 0 ? locationLike.pathname : "/";
    try {
      return new URL(`${protocol}//${locationLike.host}${pathname}`);
    } catch {
      return null;
    }
  }

  return null;
}

function isImplicitWorkstationOrigin(url: URL | null): boolean {
  if (url === null) {
    return false;
  }

  return IMPLICIT_WORKSTATION_PORTS.has(url.port);
}

function getMissingBackendErrorMessage(target: BackendTarget): string {
  if (target === "workstation") {
    return "Workstation backend endpoint is not configured for the current frontend origin.";
  }
  return "Web backend endpoint is not configured for the current frontend origin.";
}

function inferBackendBaseFromLocation(target: BackendTarget): string {
  const currentUrl = readLocationUrl();
  if (currentUrl === null) {
    return "";
  }

  if (target === "workstation") {
    return isImplicitWorkstationOrigin(currentUrl) ? trimTrailingSlash(currentUrl.origin) : "";
  }

  if (target !== "web") {
    return "";
  }

  if (currentUrl.port === "7870") {
    return trimTrailingSlash(currentUrl.origin);
  }

  currentUrl.port = "7870";
  return trimTrailingSlash(currentUrl.origin);
}

function readRuntimeApiBase(target: BackendTarget): string {
  const runtimeBases = (
    globalThis as typeof globalThis & {
      __AGENT_THE_SPIRE_API_BASES__?: RuntimeApiBases;
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
    if (target !== "same-origin" && target === "workstation") {
      throw new Error(getMissingBackendErrorMessage(target));
    }
    return path;
  }
  if (/^https?:\/\//.test(path)) {
    return path;
  }
  return path.startsWith("/") ? `${baseUrl}${path}` : `${baseUrl}/${path}`;
}

function readRuntimeWsBase(target: WebSocketTarget): string {
  const runtimeBases = (
    globalThis as typeof globalThis & {
      __AGENT_THE_SPIRE_WS_BASES__?: RuntimeWsBases;
    }
  ).__AGENT_THE_SPIRE_WS_BASES__;
  const configured = runtimeBases?.[target];
  return typeof configured === "string" ? trimTrailingSlash(configured) : "";
}

function inferWorkstationWebSocketBaseFromLocation(): string {
  const currentUrl = readLocationUrl();
  if (!isImplicitWorkstationOrigin(currentUrl) || currentUrl === null) {
    return "";
  }

  const protocol = currentUrl.protocol === "https:" ? "wss:" : DEFAULT_FRONTEND_WS_PROTOCOL;
  return trimTrailingSlash(`${protocol}//${currentUrl.host}`);
}

export function resolveWorkstationWebSocketBaseUrl(): string {
  const configuredBase = readRuntimeWsBase("workstation");
  if (configuredBase) {
    return configuredBase;
  }
  return inferWorkstationWebSocketBaseFromLocation();
}

export function buildWorkstationWebSocketUrl(path: string): string {
  const baseUrl = resolveWorkstationWebSocketBaseUrl();
  if (!baseUrl) {
    throw new Error("Workstation websocket endpoint is not configured for the current frontend origin.");
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
