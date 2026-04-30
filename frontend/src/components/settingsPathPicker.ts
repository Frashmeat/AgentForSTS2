import type { PickPathRequest } from "../shared/api/config.ts";

export type SettingsPathField = "default_project_root" | "sts2_path" | "godot_exe_path";

export function createSettingsPickPathRequest(field: SettingsPathField, currentValue?: string): PickPathRequest {
  const initial_path = currentValue?.trim() || undefined;

  if (field === "default_project_root") {
    return {
      kind: "directory",
      title: "选择默认 Mod 项目目录",
      initial_path,
    };
  }

  if (field === "sts2_path") {
    return {
      kind: "directory",
      title: "选择 STS2 游戏根目录",
      initial_path,
    };
  }

  return {
    kind: "file",
    title: "选择 Godot 4.5.1 Mono 可执行文件",
    initial_path,
    filters: [["Godot executable", "*.exe"]],
  };
}
