import test from "node:test";
import assert from "node:assert/strict";

import {
  appendWorkflowLogEntry,
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
  const next = appendWorkflowLogEntry([
    {
      text: "chunk-1",
      source: "agent",
      channel: "raw",
      model: "gpt-5.4",
    },
  ], {
    text: "chunk-2",
    source: "agent",
    channel: "raw",
    model: "gpt-5.4",
  });

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
