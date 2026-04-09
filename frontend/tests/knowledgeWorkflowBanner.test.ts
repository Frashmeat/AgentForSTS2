import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("workflow pages expose unified knowledge status banner entry points", () => {
  const batchSource = readSource("../src/features/batch-generation/view.tsx");
  const modSource = readSource("../src/features/mod-editor/view.tsx");
  const logSource = readSource("../src/features/log-analysis/view.tsx");

  assert.match(batchSource, /KnowledgeStatusBanner/);
  assert.match(batchSource, /规划与生成结果准确性可能下降/);
  assert.match(modSource, /KnowledgeStatusBanner/);
  assert.match(modSource, /分析与修改结果准确性可能下降/);
  assert.match(logSource, /KnowledgeStatusBanner/);
  assert.match(logSource, /分析结果准确性可能下降/);
});

test("App exposes a missing-knowledge confirmation dialog with guide and settings actions", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /pendingKnowledgeConfirmation/);
  assert.match(appSource, /当前未检测到可用知识库/);
  assert.match(appSource, /打开设置/);
  assert.match(appSource, /查看知识库说明/);
});
