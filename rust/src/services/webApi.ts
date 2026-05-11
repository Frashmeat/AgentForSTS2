// Web 端 API 实现 —— 必须实现 tauriApi.ts 全部导出。

import type { HealthReport, KnowledgeStatus } from "./tauriApi";

const API_BASE = "/api";

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

async function postJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, { method: "POST" });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

export function getHealth(): Promise<HealthReport> {
  return getJson<HealthReport>("/health");
}

export function getKnowledgeStatus(): Promise<KnowledgeStatus> {
  return getJson<KnowledgeStatus>("/knowledge/status");
}

export function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return postJson<KnowledgeStatus>("/knowledge/check");
}
