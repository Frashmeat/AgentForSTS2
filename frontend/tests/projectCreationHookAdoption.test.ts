import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset and mod editor features reuse shared useProjectCreation hook", () => {
  const singleAssetSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");
  const batchSource = readSource("../src/features/batch-generation/view.tsx");
  const fieldSource = readSource("../src/components/ProjectRootField.tsx");
  const hookSource = readSource("../src/shared/useProjectCreation.ts");

  assert.match(singleAssetSource, /useProjectCreation/);
  assert.match(singleAssetSource, /onStatusNotice/);
  assert.match(modEditorSource, /useProjectCreation/);
  assert.match(modEditorSource, /onStatusNotice/);
  assert.match(batchSource, /<ProjectRootField[\s\S]*onStatusNotice=\{onStatusNotice\}/);
  assert.match(fieldSource, /onStatusNotice\?\.\(/);
  assert.match(hookSource, /onStatusNotice\?\.\(/);
});
