import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel exposes knowledge status, update action and knowledge guide action", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /知识库状态/);
  assert.match(source, /更新知识库/);
  assert.match(source, /查看知识库说明/);
});

test("settings panel uses knowledge status and refresh task APIs", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /loadKnowledgeStatus/);
  assert.match(source, /checkKnowledgeStatus/);
  assert.match(source, /startRefreshKnowledgeTask/);
  assert.match(source, /getRefreshKnowledgeTask/);
});
