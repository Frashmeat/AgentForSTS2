import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("approval action flow is shared by single asset and batch features", () => {
  const singleAssetSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");
  const batchModeSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(singleAssetSource, /runApprovalAction/);
  assert.match(batchModeSource, /runApprovalAction/);
});
