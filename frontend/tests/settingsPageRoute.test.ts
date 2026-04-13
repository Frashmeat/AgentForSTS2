import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App exposes a dedicated settings page route instead of workspace drawer state", () => {
  const appSource = readSource("../src/App.tsx");
  const configSource = readSource("../src/features/workspace/config.ts");

  assert.match(appSource, /import\s+\{\s*SettingsPage\s*\}\s+from\s+"\.\/pages\/SettingsPage\.tsx";/);
  assert.match(configSource, /export function buildSettingsPath\(returnTo: string\)/);
  assert.match(appSource, /<Route path="\/settings" element=\{<SettingsPage \/>\} \/>/);
  assert.doesNotMatch(appSource, /const\s+\[settingsOpen,\s*setSettingsOpen\]\s*=\s*useState/);
});

test("settings page keeps a return link and reuses the page-mode settings panel", () => {
  const settingsPageSource = readSource("../src/pages/SettingsPage.tsx");
  const settingsPanelSource = readSource("../src/components/SettingsPanel.tsx");

  assert.equal(existsSync(new URL("../src/pages/SettingsPage.tsx", import.meta.url)), true);
  assert.match(settingsPageSource, /返回工作区/);
  assert.match(settingsPageSource, /<SettingsPanel mode="page" \/>/);
  assert.match(settingsPanelSource, /mode = "drawer"/);
  assert.match(settingsPanelSource, /if \(mode === "page"\)/);
});
