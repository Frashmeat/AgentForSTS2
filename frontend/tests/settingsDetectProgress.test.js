import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel uses detect task start, polling and cancel APIs", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /startDetectAppPaths/);
  assert.match(source, /getDetectAppPathsTask/);
  assert.match(source, /cancelDetectAppPathsTask/);
});

test("settings panel exposes detect progress state and cancel action", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /const \[detectionTaskId, setDetectionTaskId\] = useState/);
  assert.match(source, /const \[detectionStep, setDetectionStep\] = useState/);
  assert.match(source, /中断检测/);
  assert.match(source, /检测中/);
});
