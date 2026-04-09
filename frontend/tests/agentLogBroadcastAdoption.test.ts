import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("AgentLog broadcast view exposes execution briefing sections", () => {
  const source = readSource("../src/components/AgentLog.tsx");

  assert.match(source, /当前状态/);
  assert.match(source, /代码生成进度/);
  assert.match(source, /执行摘要/);
  assert.match(source, /技术细节/);
  assert.match(source, /progressLabel/);
});
