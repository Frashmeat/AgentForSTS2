import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("ModEditor wires create project action through workflow api", () => {
  const source = readSource("../src/features/mod-editor/view.tsx");

  assert.match(source, /useProjectCreation/);
  assert.match(source, /createProjectAtRoot/);
  assert.match(source, /创建项目/);
  assert.match(source, /projectCreateBusy/);
});
