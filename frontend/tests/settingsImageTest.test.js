import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel exposes image generation smoke test action", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /testImageGenerationConfig/);
  assert.match(source, /测试生图配置/);
  assert.match(source, /imageTestLoading/);
  assert.match(source, /生图配置测试成功/);
});
