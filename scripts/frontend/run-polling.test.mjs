import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({ appType: "custom", server: { middlewareMode: true } });
after(async () => vite.close());

const { waitForRun } = await vite.ssrLoadModule("/src/services/runPolling.ts");

const running = { id: "run-fixture", status: "running" };
const failed = { id: "run-fixture", status: "failed" };

test("default polling waits for the persisted terminal Run without a frontend deadline", async () => {
  const sequence = [running, running, failed];
  const updates = [];
  let elapsed = 0;

  const terminal = await waitForRun("run-fixture", (run) => updates.push(run.status), {
    readRun: async () => sequence.shift() ?? failed,
    sleep: async (delayMs) => { elapsed += delayMs; },
    now: () => elapsed,
  });

  assert.equal(terminal.status, "failed");
  assert.deepEqual(updates, ["running", "running", "failed"]);
  assert.equal(elapsed, 800);
});

test("callers may still opt into a bounded polling deadline", async () => {
  let elapsed = 0;
  let reads = 0;

  await assert.rejects(
    waitForRun("run-fixture", undefined, {
      timeoutMs: 100,
      intervalMs: 50,
      readRun: async () => { reads += 1; return running; },
      sleep: async (delayMs) => { elapsed += delayMs; },
      now: () => elapsed,
    }),
    /run polling timed out/,
  );
  assert.equal(reads, 2);
});
