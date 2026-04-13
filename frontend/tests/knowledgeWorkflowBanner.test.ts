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
  const bannerSource = readSource("../src/components/KnowledgeStatusBanner.tsx");

  assert.match(batchSource, /KnowledgeStatusBanner/);
  assert.match(modSource, /KnowledgeStatusBanner/);
  assert.match(logSource, /KnowledgeStatusBanner/);
  assert.match(bannerSource, /当前知识库信息/);
  assert.match(bannerSource, /状态说明/);
  assert.match(bannerSource, /游戏知识库/);
  assert.match(bannerSource, /Baselib 知识库/);
});

test("App no longer blocks local execution with a missing-knowledge confirmation dialog", () => {
  const appSource = readSource("../src/App.tsx");

  assert.doesNotMatch(appSource, /pendingKnowledgeConfirmation/);
  assert.doesNotMatch(appSource, /当前未检测到可用知识库/);
});
