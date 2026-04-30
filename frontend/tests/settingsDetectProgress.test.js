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

test("settings panel ignores repeated detected path values so auto-save debounce can complete", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /if \(cur\[path\[path\.length - 1\]\] === value\) \{\s*return prev;\s*\}/);
  assert.match(source, /if \(snapshot\.sts2_path\) \{\s*set\(\["sts2_path"\], snapshot\.sts2_path\);\s*\}/);
  assert.match(
    source,
    /if \(snapshot\.godot_exe_path\) \{\s*set\(\["godot_exe_path"\], snapshot\.godot_exe_path\);\s*\}/,
  );
});
