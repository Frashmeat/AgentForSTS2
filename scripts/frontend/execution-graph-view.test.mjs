import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  server: { middlewareMode: true },
});

after(async () => {
  await vite.close();
});

const { isExecutionGraphView } = await vite.ssrLoadModule(
  "/src/services/executionGraphContract.ts",
);

const base = {
  executionGraphId: "graph-fixture",
  revision: 2,
  status: "running",
  activeRunId: "run-fixture",
  previousRunId: null,
  completedNodes: 1,
  totalNodes: 3,
  currentNodeId: "item.000.single",
  currentRoleId: "mod.generate.single",
  failureCode: null,
  repairRound: 1,
  feedbackPhase: "output_contract",
  canPause: true,
  canResume: false,
  canCancel: true,
};

test("ExecutionGraphView accepts only the closed feedback phase wire", () => {
  assert.equal(isExecutionGraphView(base), true);
  assert.equal(
    isExecutionGraphView({ ...base, feedbackPhase: "generated_content" }),
    true,
  );
  assert.equal(isExecutionGraphView({ ...base, feedbackPhase: null }), true);
  assert.equal(isExecutionGraphView({ ...base, feedbackPhase: "provider_retry" }), false);
  const { feedbackPhase: _, ...missing } = base;
  assert.equal(isExecutionGraphView(missing), false);
});
