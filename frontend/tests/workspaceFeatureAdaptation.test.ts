import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("mod editor and log analysis no longer render as narrow standalone tool pages", () => {
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");
  const logAnalysisSource = readSource("../src/features/log-analysis/view.tsx");

  assert.doesNotMatch(modEditorSource, /max-w-2xl mx-auto/);
  assert.doesNotMatch(logAnalysisSource, /max-w-2xl mx-auto/);
});

test("workspace feature primary actions no longer use amber as the main accent", () => {
  const singleAssetSource = readSource("../src/features/single-asset/view.tsx");
  const batchSource = readSource("../src/features/batch-generation/view.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");
  const logAnalysisSource = readSource("../src/features/log-analysis/view.tsx");

  assert.doesNotMatch(singleAssetSource, /bg-amber-500/);
  assert.doesNotMatch(batchSource, /bg-amber-500/);
  assert.doesNotMatch(modEditorSource, /bg-amber-500/);
  assert.doesNotMatch(logAnalysisSource, /bg-amber-500/);
});
