import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("approval action flow is shared by App and BatchMode", () => {
  const appSource = readSource("../src/App.tsx");
  const batchModeSource = readSource("../src/pages/BatchMode.tsx");

  assert.match(appSource, /runApprovalAction/);
  assert.match(batchModeSource, /runApprovalAction/);
});
