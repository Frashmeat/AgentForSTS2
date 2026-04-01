export interface RequestJsonOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
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

export async function requestJson<T>(path: string, options: RequestJsonOptions = {}): Promise<T> {
  const { body, headers, method = "GET", ...rest } = options;
  const hasBody = typeof body !== "undefined";
  const response = await fetch(path, {
    ...rest,
    method,
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
