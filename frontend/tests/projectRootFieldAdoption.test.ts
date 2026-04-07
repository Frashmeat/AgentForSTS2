import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("project root interaction is shared through ProjectRootField", () => {
  const singleAssetSource = readSource("../src/features/single-asset/view.tsx");
  const batchModeSource = readSource("../src/features/batch-generation/view.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");

  assert.match(singleAssetSource, /ProjectRootField/);
  assert.match(batchModeSource, /ProjectRootField/);
  assert.match(modEditorSource, /ProjectRootField/);
});

test("ProjectRootField exposes a directory picker action", () => {
  const source = readSource("../src/components/ProjectRootField.tsx");

  assert.match(source, /pickAppPath/);
  assert.match(source, /选择目录/);
});
