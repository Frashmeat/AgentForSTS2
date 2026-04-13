import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App renders single asset feature shell from App", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /SingleAssetFeatureView/);
  assert.doesNotMatch(appSource, /import BatchMode from "\.\/pages\/BatchMode";/);
  assert.doesNotMatch(appSource, /import LogAnalysis from "\.\/pages\/LogAnalysis";/);
  assert.doesNotMatch(appSource, /import ModEditor from "\.\/pages\/ModEditor";/);
  assert.doesNotMatch(appSource, /class UnifiedWorkflowFacade/);
});

test("only BatchMode page remains as a temporary shell", () => {
  const batchPageSource = readSource("../src/pages/BatchMode.tsx");
  const batchFeatureSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(batchPageSource, /BatchGenerationFeatureView/);
  assert.match(batchPageSource, /return <BatchGenerationFeatureView \/>;/);
  assert.doesNotMatch(batchFeatureSource, /"\.\.\/\.\.\/pages\/BatchMode"/);
  assert.equal(existsSync(new URL("../src/pages/ModEditor.tsx", import.meta.url)), false);
  assert.equal(existsSync(new URL("../src/pages/LogAnalysis.tsx", import.meta.url)), false);
});

test("batch review flow exposes explicit recheck feedback and focus handling", () => {
  const batchFeatureSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(batchFeatureSource, /正在重新检查当前计划/);
  assert.match(batchFeatureSource, /复核完成：/);
  assert.match(batchFeatureSource, /ReviewFeedbackBanner/);
  assert.match(batchFeatureSource, /focusItemId/);
  assert.match(batchFeatureSource, /已定位到/);
});
