import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App uses single asset workflow controller for workflow session actions", () => {
  const source = readSource("../src/App.tsx");

  assert.match(source, /createSingleAssetWorkflowController/);
  assert.doesNotMatch(source, /createSingleAssetSocket/);
  assert.doesNotMatch(source, /socket\.send\(\{ action: "confirm"/);
  assert.doesNotMatch(source, /socket\.send\(\{ action: "select"/);
  assert.doesNotMatch(source, /socket\?\.send\(\{ action: "approve_all"/);
});
