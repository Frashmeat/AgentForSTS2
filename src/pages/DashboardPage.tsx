// Dashboard 主页：工作台，展示工程和资产卡片，底部状态栏。

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ProjectCard } from "@/components/ProjectCard";
import { SingleAssetWorkflowCard } from "@/components/SingleAssetWorkflowCard";
import { Badge, PageHero } from "@/components/ui";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { api } from "@/services/api";
import type { HealthReport } from "@/services/tauriApi";
import { useProjectStore } from "@/stores/project";

function StatusBar() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const project = useProjectStore((s) => s.project);
  useEffect(() => {
    api.getHealth().then(setHealth).catch(() => {});
  }, []);
  return (
    <div className="mt-6 pt-4" style={{ borderTop: "1px solid var(--rule-soft)" }}>
      <div className="flex flex-wrap gap-3 items-center" style={{ fontSize: "12px" }}>
        <Link to="/system?tab=config" style={{ textDecoration: "none" }}>
          <Badge variant={health?.readiness?.llmConfigured ? "ok" : "error"}>
            {health?.readiness?.llmConfigured ? "LLM" : "LLM ⚠"}
          </Badge>
        </Link>
        <Link to="/system?tab=config" style={{ textDecoration: "none" }}>
          <Badge variant={health?.readiness?.imageGenConfigured ? "ok" : "muted"}>
            {health?.readiness?.imageGenConfigured ? "ImageGen" : "ImageGen ⚠"}
          </Badge>
        </Link>
        <Link to="/system" style={{ textDecoration: "none" }}>
          <Badge variant={health?.readiness?.knowledgeReady ? "ok" : "warn"}>
            {health?.readiness?.knowledgeReady ? "知识库" : "知识库 ⚠"}
          </Badge>
        </Link>
        <Link to="/system" style={{ textDecoration: "none" }}>
          <Badge variant={project ? "ok" : "muted"}>
            {project ? project.meta.name : "无工程"}
          </Badge>
        </Link>
      </div>
    </div>
  );
}

export function DashboardPage() {
  return (
    <div>
      <PageHero
        eyebrow="cockpit · dashboard"
        title="Dashboard"
        subtitle={`Runtime · ${__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}`}
      />
      <div className="space-y-4">
        <ErrorBoundary label="Project">
          <ProjectCard />
        </ErrorBoundary>
        <ErrorBoundary label="SingleAssetWorkflow">
          <SingleAssetWorkflowCard />
        </ErrorBoundary>
      </div>
      <StatusBar />
    </div>
  );
}
