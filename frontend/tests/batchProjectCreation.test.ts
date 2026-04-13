import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("batch feature keeps project root field and loads default root from config", () => {
  const source = readSource("../src/features/batch-generation/view.tsx");

  assert.match(source, /ProjectRootField/);
  assert.match(source, /loadAppConfig/);
  assert.match(source, /default_project_root/);
  assert.match(source, /showCreateAction=\{false\}/);
});
