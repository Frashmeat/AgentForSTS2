import { AuditCard } from "@/components/AuditCard";
import { CapabilitiesCard } from "@/components/CapabilitiesCard";
import { FirstRunBanner } from "@/components/FirstRunBanner";
import { HealthCard } from "@/components/HealthCard";
import { JobsCard } from "@/components/JobsCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { PageHero } from "@/components/ui";

export function SystemPage() {
  return (
    <div>
      <PageHero
        eyebrow="operations · system"
        title="System"
        subtitle="状态监控 · 知识库 · 任务队列 · 审计日志"
      />
      <FirstRunBanner />
      <div className="space-y-4">
        <ErrorBoundary label="Health"><HealthCard /></ErrorBoundary>
        <ErrorBoundary label="Capabilities"><CapabilitiesCard /></ErrorBoundary>
        <ErrorBoundary label="Knowledge"><KnowledgeCard /></ErrorBoundary>
        <ErrorBoundary label="Jobs"><JobsCard /></ErrorBoundary>
        <ErrorBoundary label="Audit"><AuditCard /></ErrorBoundary>
      </div>
    </div>
  );
}
