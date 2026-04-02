import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("project root interaction is shared through ProjectRootField", () => {
  const singleAssetSource = readSource("../src/features/single-asset/view.tsx");
  const batchModeSource = readSource("../src/pages/BatchMode.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");

  assert.match(singleAssetSource, /ProjectRootField/);
  assert.match(batchModeSource, /ProjectRootField/);
  assert.match(modEditorSource, /ProjectRootField/);
});
