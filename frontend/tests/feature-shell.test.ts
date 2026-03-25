import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App renders single asset feature shell from App", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /SingleAssetFeatureView/);
  assert.doesNotMatch(appSource, /import BatchMode from "\.\/pages\/BatchMode";/);
  assert.doesNotMatch(appSource, /import LogAnalysis from "\.\/pages\/LogAnalysis";/);
  assert.doesNotMatch(appSource, /import ModEditor from "\.\/pages\/ModEditor";/);
});

test("legacy pages delegate to feature views", () => {
  const batchPageSource = readSource("../src/pages/BatchMode.tsx");
  const modEditorPageSource = readSource("../src/pages/ModEditor.tsx");
  const logAnalysisPageSource = readSource("../src/pages/LogAnalysis.tsx");

  assert.match(batchPageSource, /BatchGenerationFeatureView/);
  assert.match(modEditorPageSource, /ModEditorFeatureView/);
  assert.match(logAnalysisPageSource, /LogAnalysisFeatureView/);
});
