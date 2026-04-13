import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("project root loading uses the appropriate shared strategy per workspace feature", () => {
  const singleAssetSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");
  const batchModeSource = readSource("../src/features/batch-generation/view.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");

  assert.match(singleAssetSource, /useDefaultProjectRoot/);
  assert.match(batchModeSource, /loadAppConfig/);
  assert.match(modEditorSource, /useDefaultProjectRoot/);
});
