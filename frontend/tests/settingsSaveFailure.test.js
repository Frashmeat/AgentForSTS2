import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel resets saving state in a finally block when save fails", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /async function save\(\)\s*\{/);
  assert.match(source, /try\s*\{/);
  assert.match(source, /const signature = JSON\.stringify\(body\);/);
  assert.match(source, /await updateAppConfig\(body\);/);
  assert.match(source, /finally\s*\{\s*if \(mountedRef\.current\) \{\s*setSaving\(false\);/);
});

test("settings panel exposes a visible save error message", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /const \[saveError, setSaveError\] = useState/);
  assert.match(source, /const message = resolveErrorMessage\(error\) \|\| "保存设置失败";/);
  assert.match(source, /setSaveError\(message\)/);
  assert.match(source, /text-rose-600/);
});

test("settings panel auto-saves workspace changes instead of showing a manual save button", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /setTimeout\(\(\) => \{\s*void save\(\);\s*\}, 700\);/);
  assert.match(source, /已自动保存工作区设置/);
  assert.match(source, /修改后会自动保存/);
  assert.doesNotMatch(source, /\{saving \? "保存中…" : "保存设置"\}/);
});

test("settings panel auto-saves server preference changes", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /setServerSelectionDirty\(true\)/);
  assert.match(
    source,
    /setTimeout\(\(\) => \{\s*void handleSaveServerPreference\(selectedServerProfileId \?\? null\);\s*\}, 500\);/,
  );
  assert.match(source, /已自动保存默认服务器配置/);
  assert.doesNotMatch(source, /\{serverSaving \? "保存中…" : "保存默认服务器配置"\}/);
});

test("settings panel does not auto-save unavailable server profiles", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /const disabled = serverSaving \|\| !profile\.available;/);
  assert.match(source, /if \(!profile\.available\) \{\s*return;\s*\}/);
  assert.match(source, /disabled=\{disabled\}/);
  assert.match(source, /cursor-not-allowed opacity-60/);
});

test("settings panel surfaces path picker failures to the user", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /resolveErrorMessage\(error\)/);
  assert.match(source, /setPathNotes\(/);
});
