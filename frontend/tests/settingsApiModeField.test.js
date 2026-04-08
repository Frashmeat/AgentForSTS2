import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel exposes a required text model field in api mode", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /cfg\.llm\?\.mode === "api"/);
  assert.match(source, /文本模型/);
  assert.match(source, /例如 openai\/gpt-5、qwen-plus、deepseek-chat/);
  assert.match(source, /自动选择：Claude 兼容链路|自动选择：Codex 兼容链路/);
});
