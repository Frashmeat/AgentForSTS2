import test from "node:test";
import assert from "node:assert/strict";

import {
  appendBuildDeployLog,
  applyBuildDeployActionResult,
  createIdleBuildDeployState,
  describeBuildDeployCompletionView,
  describeBuildDeployAction,
  describeBuildDeployRunningMessage,
  finalizeBuildProjectResult,
  finalizeDeployResult,
  finalizePackageProjectResult,
  normalizeBuildOutputLines,
  startBuildDeployAction,
} from "../src/components/buildDeployModel.ts";

test("createIdleBuildDeployState returns cleared idle state", () => {
  assert.deepEqual(createIdleBuildDeployState(), {
    stage: "idle",
    action: null,
    log: [],
    deployedTo: null,
    summary: null,
    errorMsg: null,
  });
});

test("startBuildDeployAction resets previous feedback and enters running state", () => {
  assert.deepEqual(
    startBuildDeployAction("build"),
    {
      stage: "running",
      action: "build",
      log: [],
      deployedTo: null,
      summary: null,
      errorMsg: null,
    },
  );
});

test("normalizeBuildOutputLines trims empty lines and preserves meaningful output", () => {
  assert.deepEqual(
    normalizeBuildOutputLines("\nBuild started\n\nBuild succeeded\n"),
    ["Build started", "Build succeeded"],
  );
});

test("finalizeBuildProjectResult returns done state for successful build", () => {
  assert.deepEqual(
    finalizeBuildProjectResult({
      success: true,
      output: "Build started\nBuild succeeded\n",
    }),
    {
      stage: "done",
      summary: "构建成功",
      log: ["Build started", "Build succeeded"],
      errorMsg: null,
    },
  );
});

test("finalizeBuildProjectResult returns error state for failed build", () => {
  assert.deepEqual(
    finalizeBuildProjectResult({
      success: false,
      output: "Build failed: missing sdk",
    }),
    {
      stage: "error",
      summary: null,
      log: ["Build failed: missing sdk"],
      errorMsg: "Build failed: missing sdk",
    },
  );
});

test("finalizePackageProjectResult returns done state for successful package", () => {
  assert.deepEqual(
    finalizePackageProjectResult({
      success: true,
    }),
    {
      stage: "done",
      summary: "打包成功",
      log: [],
      errorMsg: null,
    },
  );
});

test("finalizePackageProjectResult returns error state for failed package", () => {
  assert.deepEqual(
    finalizePackageProjectResult({
      success: false,
    }),
    {
      stage: "error",
      summary: null,
      log: [],
      errorMsg: "打包失败",
    },
  );
});

test("appendBuildDeployLog appends stream output to current state", () => {
  assert.deepEqual(
    appendBuildDeployLog(startBuildDeployAction("deploy"), "packing assets"),
    {
      stage: "running",
      action: "deploy",
      log: ["packing assets"],
      deployedTo: null,
      summary: null,
      errorMsg: null,
    },
  );
});

test("finalizeDeployResult marks deploy success with deployed path", () => {
  assert.deepEqual(
    finalizeDeployResult(
      appendBuildDeployLog(startBuildDeployAction("deploy"), "done"),
      "E:/Mods/MyMod",
    ),
    {
      stage: "done",
      action: "deploy",
      log: ["done"],
      deployedTo: "E:/Mods/MyMod",
      summary: "已部署",
      errorMsg: null,
    },
  );
});

test("applyBuildDeployActionResult merges HTTP action result into state", () => {
  assert.deepEqual(
    applyBuildDeployActionResult(
      startBuildDeployAction("package"),
      finalizePackageProjectResult({ success: true }),
    ),
    {
      stage: "done",
      action: "package",
      log: [],
      deployedTo: null,
      summary: "打包成功",
      errorMsg: null,
    },
  );
});

test("describeBuildDeployRunningMessage returns dedicated deploy text", () => {
  assert.equal(
    describeBuildDeployRunningMessage("deploy"),
    "Code Agent 构建中（含 .pck 导出）…",
  );
  assert.equal(
    describeBuildDeployRunningMessage("build"),
    "仅构建中…",
  );
});

test("describeBuildDeployCompletionView returns deployed card metadata", () => {
  assert.deepEqual(
    describeBuildDeployCompletionView(
      finalizeDeployResult(startBuildDeployAction("deploy"), "E:/Mods/MyMod"),
    ),
    {
      tone: "success",
      title: "已部署",
      detail: "E:/Mods/MyMod",
      detailMonospace: true,
      showOpenSettings: false,
    },
  );
});

test("describeBuildDeployCompletionView returns deploy warning metadata when auto deploy is missing", () => {
  assert.deepEqual(
    describeBuildDeployCompletionView(
      finalizeDeployResult(startBuildDeployAction("deploy"), null),
    ),
    {
      tone: "warning",
      title: "构建成功，未自动部署",
      detail: "在设置中配置 STS2 游戏路径后可自动复制到 Mods 文件夹",
      detailMonospace: false,
      showOpenSettings: true,
    },
  );
});

test("describeBuildDeployCompletionView returns build success metadata", () => {
  assert.deepEqual(
    describeBuildDeployCompletionView(
      applyBuildDeployActionResult(
        startBuildDeployAction("build"),
        finalizeBuildProjectResult({
          success: true,
          output: "Build succeeded",
        }),
      ),
    ),
    {
      tone: "success",
      title: "构建成功",
      detail: "已完成项目构建，可继续部署或排查构建输出。",
      detailMonospace: false,
      showOpenSettings: false,
    },
  );
});

test("describeBuildDeployAction returns user-facing labels", () => {
  assert.equal(describeBuildDeployAction("deploy"), "构建并部署");
  assert.equal(describeBuildDeployAction("build"), "仅构建");
  assert.equal(describeBuildDeployAction("package"), "仅打包");
});
