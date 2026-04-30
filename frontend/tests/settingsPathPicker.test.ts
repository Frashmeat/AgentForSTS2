import test from "node:test";
import assert from "node:assert/strict";

import { createSettingsPickPathRequest, type SettingsPathField } from "../src/components/settingsPathPicker.ts";

test("createSettingsPickPathRequest returns directory picker config for project root", () => {
  assert.deepEqual(createSettingsPickPathRequest("default_project_root", "E:/STS2mod"), {
    kind: "directory",
    title: "选择默认 Mod 项目目录",
    initial_path: "E:/STS2mod",
  });
});

test("createSettingsPickPathRequest returns directory picker config for sts2 path", () => {
  assert.deepEqual(createSettingsPickPathRequest("sts2_path", "E:/steam/steamapps/common/Slay the Spire 2"), {
    kind: "directory",
    title: "选择 STS2 游戏根目录",
    initial_path: "E:/steam/steamapps/common/Slay the Spire 2",
  });
});

test("createSettingsPickPathRequest returns file picker config for godot exe", () => {
  assert.deepEqual(createSettingsPickPathRequest("godot_exe_path", "C:/tools/Godot_v4.5.1-stable_mono_win64.exe"), {
    kind: "file",
    title: "选择 Godot 4.5.1 Mono 可执行文件",
    initial_path: "C:/tools/Godot_v4.5.1-stable_mono_win64.exe",
    filters: [["Godot executable", "*.exe"]],
  });
});
