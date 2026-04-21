import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("workspace topbar exposes compact knowledge status tag", () => {
  const batchSource = readSource("../src/features/batch-generation/view.tsx");
  const modSource = readSource("../src/features/mod-editor/view.tsx");
  const logSource = readSource("../src/features/log-analysis/view.tsx");
  const shellSource = readSource("../src/components/workspace/WorkspaceShell.tsx");
  const tagSource = readSource("../src/components/KnowledgeStatusTag.tsx");

  assert.doesNotMatch(batchSource, /KnowledgeStatusBanner/);
  assert.doesNotMatch(modSource, /KnowledgeStatusBanner/);
  assert.doesNotMatch(logSource, /KnowledgeStatusBanner/);
  assert.match(shellSource, /KnowledgeStatusTag/);
  assert.match(tagSource, /知识库/);
  assert.match(tagSource, /游戏/);
  assert.match(tagSource, /Baselib/);
});

test("App no longer blocks local execution with a missing-knowledge confirmation dialog", () => {
  const appSource = readSource("../src/App.tsx");

  assert.doesNotMatch(appSource, /pendingKnowledgeConfirmation/);
  assert.doesNotMatch(appSource, /当前未检测到可用知识库/);
});
