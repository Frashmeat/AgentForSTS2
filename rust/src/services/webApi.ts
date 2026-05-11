// Web 端 API 实现 —— 必须实现 tauriApi.ts 全部导出。
//
// 后续 stage 加 API 时：① 在 tauriApi.ts 加签名 ② 在 webApi.ts 加 fetch 实现
// ③ TS 编译期会校验 ApiModule 类型一致。

import type { HealthReport } from "./tauriApi";

const API_BASE = "/api";

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

export function getHealth(): Promise<HealthReport> {
  return getJson<HealthReport>("/health");
}
