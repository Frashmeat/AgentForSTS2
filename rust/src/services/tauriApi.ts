// Tauri 端 API 实现 —— 签名权威，webApi.ts 必须实现同名同签名的所有导出。

import { invoke } from "@tauri-apps/api/core";

// -------- Health --------

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

// -------- Knowledge --------

export type OverallState = "fresh" | "missing" | "stale";
export type SourceMode = "runtime_decompiled" | "missing";

export interface GameStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface BaselibStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface KnowledgeStatus {
  overall: OverallState;
  knowledgeRoot: string;
  warnings: string[];
  game: GameStatus;
  baselib: BaselibStatus;
  embeddedTemplates: string[];
}

export function getKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("get_knowledge_status");
}

export function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("check_knowledge_status");
}
