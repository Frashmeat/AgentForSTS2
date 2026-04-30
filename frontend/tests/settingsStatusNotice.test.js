import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("status notice component exposes inline card and floating stack variants", () => {
  const source = readSource("../src/components/StatusNotice.tsx");

  assert.match(source, /export function StatusNotice\(/);
  assert.match(source, /export function StatusNoticeStack\(/);
  assert.match(source, /pointer-events-none fixed right-4 top-4 z-\[70\]/);
  assert.match(source, /floating \? "w-\[min\(24rem,calc\(100vw-2rem\)\)\]"/);
});

test("settings panel routes dynamic status prompts through the shared notice stack", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(
    source,
    /import \{ StatusNotice, StatusNoticeStack, type StatusNoticeItem \} from "\.\/StatusNotice\.tsx";/,
  );
  assert.match(source, /const floatingNoticeCandidates: Array<StatusNoticeItem \| null> = \[/);
  assert.match(source, /<StatusNoticeStack notices=\{floatingNotices\} \/>/);
  assert.match(source, /id: "workspace-save"/);
  assert.match(source, /id: "path-detect"/);
  assert.match(source, /id: "knowledge-status"/);
});

test("settings panel replaces inline warning blocks with shared status notices", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /<StatusNotice\s+title="服务器模式暂不可用"/);
  assert.match(source, /<StatusNotice\s+title="登录后可管理服务器模式"/);
  assert.match(source, /<StatusNotice\s+title="默认服务器配置已不可用"/);
  assert.doesNotMatch(source, /平台队列 Worker 诊断失败/);
  assert.doesNotMatch(source, /pathNotes\.map/);
  assert.doesNotMatch(source, /knowledgeNotes\.map/);
});
