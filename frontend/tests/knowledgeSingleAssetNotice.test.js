import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset view exposes knowledge notice entry points", () => {
  const source = readSource("../src/features/single-asset/view.tsx");

  assert.match(source, /KnowledgeStatusBanner/);
  assert.match(source, /生成结果准确性可能下降/);
  assert.match(source, /onOpenKnowledgeGuide/);
  assert.match(source, /onOpenSettings/);
});
