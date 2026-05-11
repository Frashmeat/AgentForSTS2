// Tauri 端 API 实现 —— 签名权威，webApi.ts 必须实现同名同签名的所有导出。

import { invoke } from "@tauri-apps/api/core";

export type Role = "web" | "workstation";

export interface ConfigStatus {
  path: string | null;
  filePresent: boolean;
  loaded: boolean;
  errors: string[];
}

export interface HealthReport {
  status: "ok" | "degraded";
  role: Role;
  coreVersion: string;
  serverTime: string;
  config: ConfigStatus;
}

export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}
