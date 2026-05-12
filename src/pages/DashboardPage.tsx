// Dashboard 主页：原来 App.tsx 里 stack 的所有 Card，保持原状但放进 router。

import { AuditCard } from "@/components/AuditCard";
import { CapabilitiesCard } from "@/components/CapabilitiesCard";
import { CodegenCard } from "@/components/CodegenCard";
import { HealthCard } from "@/components/HealthCard";
import { JobsCard } from "@/components/JobsCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";
import { ProjectCard } from "@/components/ProjectCard";
import { SingleAssetWorkflowCard } from "@/components/SingleAssetWorkflowCard";

export function DashboardPage() {
  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-semibold">Dashboard</h1>
        <p className="text-muted text-sm">
          Runtime: {__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}
        </p>
      </header>

      <HealthCard />
      <CapabilitiesCard />
      <ProjectCard />
      <SingleAssetWorkflowCard />
      <JobsCard />
      <AuditCard />
      <KnowledgeCard />
      <PlanningCard />
      <CodegenCard />
      <LlmCard />
    </div>
  );
}
