// Dashboard 主页：原来 App.tsx 里 stack 的所有 Card，放进路由后保持原状。

import { AuditCard } from "@/components/AuditCard";
import { CapabilitiesCard } from "@/components/CapabilitiesCard";
import { CodegenCard } from "@/components/CodegenCard";
import { FirstRunBanner } from "@/components/FirstRunBanner";
import { HealthCard } from "@/components/HealthCard";
import { JobsCard } from "@/components/JobsCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";
import { ProjectCard } from "@/components/ProjectCard";
import { SingleAssetWorkflowCard } from "@/components/SingleAssetWorkflowCard";
import { PageHero } from "@/components/ui";

export function DashboardPage() {
  return (
    <div>
      <PageHero
        eyebrow="cockpit · dashboard"
        title="Dashboard"
        subtitle={`Runtime · ${__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}`}
      />

      <FirstRunBanner />

      <div className="space-y-4">
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
    </div>
  );
}
