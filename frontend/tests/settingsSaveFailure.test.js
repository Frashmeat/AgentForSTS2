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
  assert.match(source, /await updateAppConfig\(body\);/);
  assert.match(source, /finally\s*\{\s*setSaving\(false\);/);
});

test("settings panel exposes a visible save error message", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /const \[saveError, setSaveError\] = useState/);
  assert.match(source, /setSaveError\(resolveErrorMessage\(error\) \|\| "保存设置失败"\)/);
  assert.match(source, /text-rose-600/);
});

test("settings panel surfaces path picker failures to the user", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /resolveErrorMessage\(error\)/);
  assert.match(source, /setPathNotes\(/);
});
