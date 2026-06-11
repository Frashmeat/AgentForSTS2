// LLM / codegen / planning 调试工具页。
// 这些卡片对普通用户无直接价值，从 Dashboard 移到这里避免干扰主工作流。

import { CodegenCard } from "@/components/CodegenCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { PageHero } from "@/components/ui";

export function DevToolsPage() {
  return (
    <div>
      <PageHero
        eyebrow="devtools"
        title="Developer Tools"
        subtitle="LLM prompt preview · plan validation · playground"
      />

      <div className="space-y-4">
        <ErrorBoundary label="Planning">
          <PlanningCard />
        </ErrorBoundary>

        <ErrorBoundary label="Codegen">
          <CodegenCard />
        </ErrorBoundary>

        <ErrorBoundary label="LLM">
          <LlmCard />
        </ErrorBoundary>
      </div>
    </div>
  );
}
