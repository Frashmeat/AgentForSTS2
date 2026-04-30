import test from "node:test";
import assert from "node:assert/strict";

import {
  appendWorkflowLogEntry,
  buildCodegenBroadcastView,
  buildPrettyWorkflowLogLines,
  buildRawWorkflowLogLines,
  resolveNextWorkflowModel,
  type WorkflowLogEntry,
} from "../src/shared/workflowLog.ts";

test("appendWorkflowLogEntry keeps original text and metadata", () => {
  const next = appendWorkflowLogEntry([], {
    text: "chunk-1",
    source: "agent",
    channel: "stderr",
    model: "gpt-5.4",
  });

  assert.deepEqual(next, [
    {
      text: "chunk-1",
      source: "agent",
      channel: "stderr",
      model: "gpt-5.4",
    },
  ]);
});

test("appendWorkflowLogEntry merges adjacent chunks from the same stream", () => {
  const next = appendWorkflowLogEntry(
    [
      {
        text: "chunk-1",
        source: "agent",
        channel: "raw",
        model: "gpt-5.4",
      },
    ],
    {
      text: "chunk-2",
      source: "agent",
      channel: "raw",
      model: "gpt-5.4",
    },
  );

  assert.deepEqual(next, [
    {
      text: "chunk-1chunk-2",
      source: "agent",
      channel: "raw",
      model: "gpt-5.4",
    },
  ]);
});

test("resolveNextWorkflowModel keeps previous model when new entry has no model", () => {
  const entry: WorkflowLogEntry = { text: "build started", source: "build", channel: "raw" };
  assert.equal(resolveNextWorkflowModel("claude-sonnet-4-6", entry), "claude-sonnet-4-6");
});

test("buildRawWorkflowLogLines preserves insertion order", () => {
  const lines = buildRawWorkflowLogLines([
    { text: "line-1", source: "agent", channel: "raw" },
    { text: "line-2", source: "agent", channel: "stderr" },
  ]);

  assert.deepEqual(lines, ["line-1", "line-2"]);
});

test("buildPrettyWorkflowLogLines highlights stderr and collapses adjacent duplicates", () => {
  const lines = buildPrettyWorkflowLogLines([
    { text: "阶段一", source: "workflow", channel: "stage" },
    { text: "阶段一", source: "workflow", channel: "stage" },
    { text: "boom", source: "agent", channel: "stderr" },
  ]);

  assert.deepEqual(lines, ["阶段一", "[stderr] boom"]);
});

test("buildPrettyWorkflowLogLines summarizes build deploy output without leaking raw command spam", () => {
  const lines = buildPrettyWorkflowLogLines([
    {
      text:
        "[Bash] dotnet publish [Glob] **/*.sln [Read] I:\\WebCode\\STS2ModProject\\STS2ModProject.sln 解决方案文件看起来正常。\n" +
        "让我尝试直接在项目目录中运行 `dotnet publish`: [Bash] dotnet publish STS2ModProject.csproj `dotnet publish` **成功完成**（第 1 次尝试）。\n" +
        "**构建结果:** - ✅ DLL 编译成功: `.godot/mono/temp/bin/Release/STS2ModProject.dll` - ✅ .pck 文件已导出到 mods 文件夹 - ⚠️ 1 个警告: `FangedGrimoire.cs(13,21): warning STS003 - Model 应继承 BaseLib.Abstracts.CustomRelicModel 或添加接口 ICustomModel`\n" +
        "**输出位置:** - DLL: `I:\\WebCode\\STS2ModProject\\.godot\\mono\\temp\\bin\\Release\\STS2ModProject.dll` - 发布目录: `I:\\WebCode\\STS2ModProject\\.godot\\mono\\temp\\bin\\Release\\publish\\` - Pck 文件已复制到 mods 文件夹\n" +
        "`dotnet publish` **成功完成**（第 1 次尝试）。 **构建结果:** - ✅ DLL 编译成功: `.godot/mono/temp/bin/Release/STS2ModProject.dll` - ✅ .pck 文件已导出到 mods 文件夹",
      source: "build",
      channel: "raw",
      model: "Codex CLI 默认模型",
    },
  ]);

  assert.deepEqual(lines, [
    "构建成功：dotnet publish 已完成。",
    "DLL 编译成功：.godot/mono/temp/bin/Release/STS2ModProject.dll",
    "PCK 文件已导出到 mods 文件夹。",
    "构建警告：1 个警告，包含 warning STS003。",
    "发布目录：I:\\WebCode\\STS2ModProject\\.godot\\mono\\temp\\bin\\Release\\publish\\",
  ]);
  assert.equal(
    lines.some((line) => line.includes("[Bash] dotnet publish")),
    false,
  );
});

test("buildCodegenBroadcastView starts from task understanding before agent output arrives", () => {
  const view = buildCodegenBroadcastView([], {
    currentStage: "正在生成代码...",
    isComplete: false,
  });

  assert.equal(view.currentStep.index, 1);
  assert.equal(view.completedCount, 0);
  assert.equal(view.progressLabel, "0 / 6 已完成");
  assert.match(view.currentStatus, /理解需求与上下文/);
  assert.deepEqual(
    view.steps.map((step) => step.status),
    ["current", "pending", "pending", "pending", "pending", "pending"],
  );
});

test("buildCodegenBroadcastView derives codegen summary from structured progress hints", () => {
  const view = buildCodegenBroadcastView(
    [
      { text: "我先扫描项目结构和关键文件。", source: "agent", channel: "raw", model: "gpt-5" },
      { text: "已定位到主要改动点，准备整理修改方案。", source: "agent", channel: "raw", model: "gpt-5" },
    ],
    {
      currentStage: "正在生成代码...",
      isComplete: false,
    },
  );

  assert.equal(view.currentStep.index, 4);
  assert.equal(view.completedCount, 3);
  assert.equal(view.progressLabel, "3 / 6 已完成");
  assert.match(view.currentStatus, /当前进入修改方案整理阶段/);
  assert.deepEqual(view.summaryLines, [
    "已完成任务理解、项目扫描与改动点定位。",
    "当前正根据上下文整理本轮修改方案。",
    "暂未进入代码写入与自检收尾阶段。",
  ]);
  assert.deepEqual(view.detailLines, ["我先扫描项目结构和关键文件。", "已定位到主要改动点，准备整理修改方案。"]);
});

test("buildCodegenBroadcastView marks all steps complete when agent run is finished", () => {
  const view = buildCodegenBroadcastView(
    [{ text: "已完成代码写入，开始自检。", source: "agent", channel: "raw", model: "gpt-5" }],
    {
      currentStage: "构建完成",
      isComplete: true,
    },
  );

  assert.equal(view.currentStep.index, 6);
  assert.equal(view.completedCount, 6);
  assert.equal(view.progressLabel, "6 / 6 已完成");
  assert.match(view.currentStatus, /已完成代码生成与自检收尾/);
  assert.ok(view.steps.every((step) => step.status === "done"));
});
