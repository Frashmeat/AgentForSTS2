import { HashRouter, Route, Routes } from "react-router-dom";

import { Layout } from "@/components/Layout";
import { BatchGenerationPage } from "@/pages/BatchGenerationPage";
import { DashboardPage } from "@/pages/DashboardPage";
import { LogAnalysisPage } from "@/pages/LogAnalysisPage";
import { ModEditorPage } from "@/pages/ModEditorPage";
import { SettingsPage } from "@/pages/SettingsPage";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<DashboardPage />} />
          <Route path="editor" element={<ModEditorPage />} />
          <Route path="batch" element={<BatchGenerationPage />} />
          <Route path="log" element={<LogAnalysisPage />} />
          <Route path="settings" element={<SettingsPage />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
