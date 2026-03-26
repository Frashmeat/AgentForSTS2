import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("frontend config no longer exposes migration flag helpers", () => {
  const configSource = readSource("../src/shared/api/config.ts");
  const appSource = readSource("../src/App.tsx");

  assert.doesNotMatch(configSource, /resolveMigrationFlags/);
  assert.doesNotMatch(configSource, /WorkflowMigrationFlags/);
  assert.doesNotMatch(appSource, /resolveMigrationFlags/);
  assert.doesNotMatch(appSource, /use_unified_ws_contract/);
});
