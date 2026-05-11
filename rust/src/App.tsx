import { CodegenCard } from "@/components/CodegenCard";
import { HealthCard } from "@/components/HealthCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";

export default function App() {
  return (
    <div className="min-h-screen p-8 max-w-3xl space-y-6">
      <header>
        <h1 className="text-2xl font-semibold">AgentTheSpire</h1>
        <p className="text-muted mt-1">
          Rust + Tauri rewrite — version {__APP_VERSION__}
        </p>
        <p className="text-muted text-sm">
          Runtime: {__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}
        </p>
      </header>

      <HealthCard />
      <KnowledgeCard />
      <PlanningCard />
      <CodegenCard />
      <LlmCard />
    </div>
  );
}
