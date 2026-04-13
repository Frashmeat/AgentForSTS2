import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset and mod editor features reuse shared useProjectCreation hook", () => {
  const singleAssetSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");

  assert.match(singleAssetSource, /useProjectCreation/);
  assert.match(modEditorSource, /useProjectCreation/);
});
