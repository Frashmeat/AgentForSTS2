import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("knowledge guide dialog exposes actual source labels and paths", () => {
  const source = readSource("../src/components/KnowledgeGuideDialog.tsx");

  assert.match(source, /当前实际使用来源/);
  assert.match(source, /当前知识路径/);
  assert.match(source, /runtime_decompiled/);
  assert.match(source, /repo_reference/);
  assert.match(source, /repo_fallback/);
});
