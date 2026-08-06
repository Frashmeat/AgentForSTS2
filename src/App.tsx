import { HashRouter, Route, Routes } from "react-router-dom";

import { Layout } from "@/components/Layout";
import { BatchGenerationPage } from "@/pages/BatchGenerationPage";
import { CompositionStudioPage } from "@/pages/CompositionStudioPage";
import { DashboardPage } from "@/pages/DashboardPage";
import { LogAnalysisPage } from "@/pages/LogAnalysisPage";
import { ModEditorPage } from "@/pages/ModEditorPage";
import { DevToolsPage } from "@/pages/DevToolsPage";
import { SystemPage } from "@/pages/SystemPage";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<DashboardPage />} />
          <Route path="system" element={<SystemPage />} />
          <Route path="editor" element={<ModEditorPage />} />
          <Route path="composition" element={<CompositionStudioPage />} />
          <Route path="batch" element={<BatchGenerationPage />} />
          <Route path="log" element={<LogAnalysisPage />} />
          <Route path="runs" element={<DevToolsPage />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
