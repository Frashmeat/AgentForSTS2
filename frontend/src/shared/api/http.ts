export interface RequestJsonOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
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
