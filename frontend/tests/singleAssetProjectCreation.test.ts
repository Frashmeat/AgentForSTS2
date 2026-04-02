import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("SingleAssetFeatureView exposes create project action", () => {
  const source = readSource("../src/features/single-asset/view.tsx");

  assert.match(source, /创建项目/);
  assert.match(source, /onCreateProject/);
  assert.match(source, /projectCreateBusy/);
});

test("App wires single asset create project handler through shared hook", () => {
  const source = readSource("../src/App.tsx");

  assert.match(source, /useProjectCreation/);
  assert.match(source, /createProjectAtRoot/);
  assert.match(source, /onCreateProject/);
});
