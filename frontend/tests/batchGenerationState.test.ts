import test from "node:test";
import assert from "node:assert/strict";

import {
  appendBatchItemAgentLog,
  appendBatchItemProgress,
  applyBatchItemImage,
  applyBatchItemStageMessage,
  createBatchItemStateRecord,
  createDefaultBatchItemState,
  updateBatchItemStateRecord,
} from "../src/features/batch-generation/state.ts";

test("createBatchItemStateRecord initializes all items with default state", () => {
  const record = createBatchItemStateRecord([{ id: "item-1" }, { id: "item-2" }]);

  assert.deepEqual(record["item-1"], createDefaultBatchItemState());
  assert.deepEqual(record["item-2"], createDefaultBatchItemState());
});

test("updateBatchItemStateRecord merges patch onto target item", () => {
  const next = updateBatchItemStateRecord({ "item-1": createDefaultBatchItemState() }, "item-1", {
    status: "done",
    currentPrompt: "prompt",
  });

  assert.equal(next["item-1"].status, "done");
  assert.equal(next["item-1"].currentPrompt, "prompt");
});

test("append helpers preserve history and logs", () => {
  let next = appendBatchItemProgress({ "item-1": createDefaultBatchItemState() }, "item-1", "planning");
  next = appendBatchItemAgentLog(next, "item-1", { text: "chunk" });
  next = applyBatchItemStageMessage(next, "item-1", "阶段一");

  assert.deepEqual(next["item-1"].progress, ["planning"]);
  assert.deepEqual(next["item-1"].agentLog, ["chunk"]);
  assert.equal(next["item-1"].currentStage, "阶段一");
  assert.deepEqual(next["item-1"].stageHistory, ["阶段一"]);
});

test("applyBatchItemImage writes image by index and enters awaiting selection", () => {
  const next = applyBatchItemImage({ "item-1": createDefaultBatchItemState() }, "item-1", "img-b64", 0, "new prompt");

  assert.equal(next["item-1"].status, "awaiting_selection");
  assert.deepEqual(next["item-1"].images, ["img-b64"]);
  assert.equal(next["item-1"].currentPrompt, "new prompt");
});
