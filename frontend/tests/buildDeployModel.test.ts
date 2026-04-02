import test from "node:test";
import assert from "node:assert/strict";

import {
  appendBuildDeployLog,
  applyBuildDeployActionResult,
  createIdleBuildDeployState,
  describeBuildDeployAction,
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

test("describeBuildDeployAction returns user-facing labels", () => {
  assert.equal(describeBuildDeployAction("deploy"), "构建并部署");
  assert.equal(describeBuildDeployAction("build"), "仅构建");
  assert.equal(describeBuildDeployAction("package"), "仅打包");
});
