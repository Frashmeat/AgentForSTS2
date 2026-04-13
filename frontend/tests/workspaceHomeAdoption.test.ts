import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App delegates workspace content assembly to WorkspaceHome", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /WorkspaceHome/);
  assert.match(appSource, /WorkspaceProvider/);
  assert.doesNotMatch(appSource, /function renderWorkspaceContent/);
  assert.doesNotMatch(appSource, /BatchGenerationFeatureView/);
  assert.doesNotMatch(appSource, /ModEditorFeatureView/);
  assert.doesNotMatch(appSource, /LogAnalysisFeatureView/);
});

test("WorkspaceHome owns tab-specific feature composition", () => {
  const source = readSource("../src/features/workspace/WorkspaceHome.tsx");

  assert.match(source, /BatchGenerationFeatureView/);
  assert.match(source, /ModEditorFeatureView/);
  assert.match(source, /LogAnalysisFeatureView/);
  assert.match(source, /SingleAssetWorkspaceContainer/);
  assert.match(source, /activeTab === "single"/);
});
