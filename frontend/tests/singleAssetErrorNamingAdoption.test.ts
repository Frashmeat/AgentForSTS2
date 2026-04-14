import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset view and container use explicit error field names", () => {
  const viewSource = readSource("../src/features/single-asset/view.tsx");
  const containerSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");

  assert.match(viewSource, /\berrorMessage\b/);
  assert.match(viewSource, /\berrorTraceback\b/);
  assert.match(containerSource, /errorMessage=\{workflowState\.errorMessage\}/);
  assert.match(containerSource, /errorTraceback=\{workflowState\.errorTraceback\}/);
  assert.doesNotMatch(viewSource, /\berrorMsg\b/);
  assert.doesNotMatch(viewSource, /\berrorTrace\b/);
});
