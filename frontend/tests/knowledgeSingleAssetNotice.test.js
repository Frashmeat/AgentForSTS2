import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset view no longer renders large knowledge status banner", () => {
  const source = readSource("../src/features/single-asset/view.tsx");
  const tagSource = readSource("../src/components/KnowledgeStatusTag.tsx");

  assert.doesNotMatch(source, /KnowledgeStatusBanner/);
  assert.match(tagSource, /知识库/);
  assert.match(tagSource, /查看知识库说明/);
});
