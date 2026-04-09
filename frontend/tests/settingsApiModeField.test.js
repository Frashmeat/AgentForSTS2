import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel exposes a required text model field in api mode", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /cfg\.llm\?\.mode === "claude_api"/);
  assert.match(source, /Claude 模型/);
  assert.match(source, /例如 claude-sonnet-4-6/);
  assert.match(source, /自动选择：Claude API/);
  assert.doesNotMatch(source, /API 提供商/);
  assert.doesNotMatch(source, /Codex 兼容链路/);
});
