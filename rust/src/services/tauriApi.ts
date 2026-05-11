// Tauri 端 API 实现 —— 签名权威，webApi.ts 必须实现同名同签名的所有导出。
//
// Stage 0：只暴露 getHealth。后续 stage 每加一个 #[tauri::command] 都在此处
// 加一条对应导出。

import { invoke } from "@tauri-apps/api/core";

export type Role = "web" | "workstation";

export interface HealthReport {
  status: "ok";
  role: Role;
  coreVersion: string;
  serverTime: string;
}

export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}
