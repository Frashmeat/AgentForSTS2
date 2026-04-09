import { BatchGenerationFeatureView } from "../features/batch-generation/view";

export default function BatchMode() {
  return <BatchGenerationFeatureView knowledgeStatus={null} onOpenKnowledgeGuide={() => {}} onOpenSettings={() => {}} />;
}
