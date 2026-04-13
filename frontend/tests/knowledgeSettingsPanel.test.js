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

test("settings panel exposes knowledge source labels for game and baselib", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /游戏知识来源/);
  assert.match(source, /Baselib 知识来源/);
  assert.match(source, /source_mode/);
});

test("settings panel exposes knowledge progress bars for check and refresh flows", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /检查进度/);
  assert.match(source, /更新进度/);
  assert.match(source, /ProgressBar/);
  assert.match(source, /knowledgeChecking/);
});
