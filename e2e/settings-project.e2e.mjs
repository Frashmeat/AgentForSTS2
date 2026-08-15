import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";

import { browser, $, $$ } from "@wdio/globals";

const requiredEnv = (name) => {
  const value = process.env[name];
  assert.ok(value, `${name} must be configured by the E2E runner`);
  return path.resolve(value);
};

const navigate = async (hash) => {
  await browser.execute((nextHash) => {
    window.location.hash = nextHash;
  }, hash);
};

const waitForTestId = async (testId, timeout = 15_000) => {
  await browser.waitUntil(
    async () => browser.execute(
      (id) => Boolean(document.querySelector(`[data-testid="${id}"]`)),
      testId,
    ),
    { timeout, timeoutMsg: `${testId} did not appear` },
  );
};

const selectValue = async (testId, value, timeout = 30_000) => {
  try {
    await browser.waitUntil(
      async () => browser.execute((id, nextValue) => {
        const element = document.querySelector(`[data-testid="${id}"]`);
        if (!(element instanceof HTMLSelectElement)) return false;
        const option = Array.from(element.options).find((candidate) => candidate.value === nextValue);
        return Boolean(option && !option.disabled);
      }, testId, value),
      { timeout, timeoutMsg: `${testId} option ${value} did not become ready` },
    );
  } catch {
    const state = await browser.execute((id) => {
      const element = document.querySelector(`[data-testid="${id}"]`);
      const options = element instanceof HTMLSelectElement
        ? Array.from(element.options).map((option) => ({ value: option.value, disabled: option.disabled }))
        : [];
      return {
        options,
        error: document.querySelector('[data-testid="item-editor-error"]')?.textContent?.trim() ?? "",
      };
    }, testId);
    throw new Error(`${testId} option ${value} did not become ready: ${JSON.stringify(state)}`);
  }
  const selected = await browser.execute((id, nextValue) => {
    const element = document.querySelector(`[data-testid="${id}"]`);
    if (!(element instanceof HTMLSelectElement)) return null;
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
    setter?.call(element, nextValue);
    element.dispatchEvent(new Event("change", { bubbles: true }));
    return element.value;
  }, testId, value);
  assert.equal(selected, value, `${testId} did not select ${value}`);
};

const waitForJsonFile = async (filePath, predicate, timeout = 30_000) => browser.waitUntil(
  async () => {
    try {
      const value = JSON.parse(await fs.readFile(filePath, "utf8"));
      return predicate(value) ? value : false;
    } catch {
      return false;
    }
  },
  { timeout, timeoutMsg: `${filePath} did not reach the expected state` },
);

const waitForBatchRun = async (status, timeout = 240_000, previousRunId = null) => {
  let result = null;
  await browser.waitUntil(
    async () => browser.execute((expectedStatus, oldRunId) => {
      const element = document.querySelector('[data-testid="batch-run-result"]');
      const id = element?.getAttribute("data-run-id") ?? null;
      if (id === oldRunId || element?.getAttribute("data-run-status") !== expectedStatus) return null;
      return {
        id,
        status: element.getAttribute("data-run-status"),
      };
    }, status, previousRunId).then((value) => {
      result = value;
      return Boolean(value?.id);
    }),
    { timeout, timeoutMsg: `Batch page Run did not reach ${status}` },
  );
  return result;
};

const waitForCompositionGraph = async (
  status,
  {
    timeout = 180_000,
    graphId = null,
    previousRunId = null,
    runStatus = null,
  } = {},
) => {
  let result = null;
  await browser.waitUntil(
    async () => browser.execute((expectedStatus, expectedGraphId, oldRunId, expectedRunStatus) => {
      const error = document.querySelector('[data-testid="composition-error"]');
      if (error?.textContent?.trim()) return { error: error.textContent.trim() };
      const element = document.querySelector('[data-testid="composition-execution-graph"]');
      if (!element || element.getAttribute("data-execution-status") !== expectedStatus) return null;
      const currentGraphId = element.getAttribute("data-execution-graph-id");
      const runId = element.getAttribute("data-run-id");
      const currentRunStatus = element.getAttribute("data-run-status");
      if (expectedGraphId && currentGraphId !== expectedGraphId) return null;
      if (!runId || runId === oldRunId) return null;
      if (expectedRunStatus && currentRunStatus !== expectedRunStatus) return null;
      return {
        graphId: currentGraphId,
        status: expectedStatus,
        runId,
        runStatus: currentRunStatus,
        completedNodes: Number(element.getAttribute("data-completed-nodes")),
        totalNodes: Number(element.getAttribute("data-total-nodes")),
      };
    }, status, graphId, previousRunId, runStatus).then((value) => {
      if (value?.error) throw new Error(`Composition submission failed: ${value.error}`);
      result = value;
      return Boolean(value?.graphId && value?.runId);
    }),
    { timeout, timeoutMsg: `Composition graph did not reach ${status}` },
  );
  return result;
};

const waitForEnabled = async (testId, timeout = 30_000) => browser.waitUntil(
  async () => browser.execute((id) => {
    const element = document.querySelector(`[data-testid="${id}"]`);
    return element instanceof HTMLButtonElement && !element.disabled;
  }, testId),
  { timeout, timeoutMsg: `${testId} did not become enabled` },
);

const resourceActionCount = async (label) => browser.execute((expectedLabel) =>
  Array.from(document.querySelectorAll("button"))
    .filter((button) => button.textContent?.trim() === expectedLabel).length,
label);

const bindRequiredResourceCandidates = async (expectedCount, timeout = 180_000) => {
  await browser.waitUntil(
    async () => (await resourceActionCount("Select")) >= expectedCount,
    { timeout, timeoutMsg: `${expectedCount} resource candidates did not become selectable` },
  );
  for (let index = 0; index < expectedCount; index += 1) {
    const before = await resourceActionCount("Bound");
    const clicked = await browser.execute(() => {
      const button = Array.from(document.querySelectorAll("button"))
        .find((candidate) => candidate.textContent?.trim() === "Select" && !candidate.disabled);
      if (!(button instanceof HTMLButtonElement)) return false;
      button.click();
      return true;
    });
    assert.equal(clicked, true, "a required Resource candidate was not selectable");
    await browser.waitUntil(
      async () => (await resourceActionCount("Bound")) === before + 1,
      { timeout, timeoutMsg: "selected Resource candidate did not become bound" },
    );
  }
};

const readStubRequests = async (root) => (await fs.readFile(
  path.join(root, "stub-requests.jsonl"),
  "utf8",
)).trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));

const assertNoAtomicWriteResidue = async (projectRoot) => {
  for (const relativeRoot of [
    ".ats/execution-graphs-v4",
    ".ats/composition-drafts-v2",
    ".ats/runs-v3",
  ]) {
    const entries = await fs.readdir(path.join(projectRoot, relativeRoot)).catch(() => []);
    assert.ok(
      entries.every((entry) => !entry.endsWith(".tmp") && !entry.endsWith(".bak") && !entry.startsWith(".staging")),
      `${relativeRoot} retained atomic-write residue: ${entries.join(", ")}`,
    );
  }
};

const sha256File = async (filePath) => createHash("sha256")
  .update(await fs.readFile(filePath))
  .digest("hex");

const assertNoTransactionResidue = async (projectRoot) => {
  const transactionRoot = path.join(projectRoot, ".ats", "transactions");
  const entries = await fs.readdir(transactionRoot).catch(() => []);
  assert.deepEqual(entries, []);
};

describe("current desktop Stage 2 workflow", () => {
  it("persists settings and creates an isolated project", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const configPath = requiredEnv("SPIREFORGE_CONFIG_PATH");
    const appDataRoot = requiredEnv("SPIREFORGE_APP_DATA_ROOT");
    const godotPath = requiredEnv("ATS_E2E_GODOT_PATH");
    const sts2Path = requiredEnv("ATS_E2E_STS2_DLL_PATH");
    const projectsRoot = path.join(root, "projects");
    const projectRoot = path.join(projectsRoot, "E2EMod");

    await navigate("/system");
    await waitForTestId("godot-exe-path");
    await $('[data-testid="godot-exe-path"]').setValue(godotPath);
    await $('[data-testid="settings-save"]').click();
    const persisted = await waitForJsonFile(
      configPath,
      (value) => value.toolchain?.godot_exe_path === godotPath,
    );
    assert.equal(persisted.toolchain.godot_exe_path, godotPath);

    await navigate("/");
    await waitForTestId("project-parent-dir");
    await $('[data-testid="project-parent-dir"]').setValue(projectsRoot);
    await $('[data-testid="project-name"]').setValue("E2EMod");
    await $('[data-testid="project-create-submit"]').click();
    await waitForTestId("project-current", 60_000);
    assert.match(await $('[data-testid="project-current"]').getText(), /E2EMod/);

    const recents = JSON.parse(await fs.readFile(
      path.join(appDataRoot, "recent_projects.json"),
      "utf8",
    ));
    assert.equal(recents.items[0].path, projectRoot);
    const deliveryPath = `${path.join(projectRoot, "delivery")}${path.sep}`;
    const localProps = [
      "<Project>",
      "  <PropertyGroup>",
      `    <Sts2AssemblyPath>${sts2Path}</Sts2AssemblyPath>`,
      `    <GodotPath>${godotPath}</GodotPath>`,
      `    <ModsPath>${deliveryPath}</ModsPath>`,
      "  </PropertyGroup>",
      "</Project>",
      "",
    ].join("\n");
    await fs.writeFile(path.join(projectRoot, "local.props"), localProps, "utf8");
    assert.match(localProps, /<GodotPath>[^<]*Godot_v4\.5\.1[^<]*<\/GodotPath>/);
  });

  it("imports a verified Truth Snapshot outside the project workspace", async () => {
    const appDataRoot = requiredEnv("SPIREFORGE_APP_DATA_ROOT");

    await navigate("/");
    await waitForTestId("truth-status");
    if ((await $('[data-testid="truth-status"]').getText()) !== "ready") {
      await $('[data-testid="truth-import"]').click();
    }
    const truthOutcome = await browser.waitUntil(
      async () => browser.execute(() => {
        const status = document.querySelector('[data-testid="truth-status"]')?.textContent;
        if (status === "ready") return { ready: true, error: "" };
        const error = document.querySelector('[data-testid="dashboard-error"]')?.textContent ?? "";
        return error ? { ready: false, error } : null;
      }),
      { timeout: 180_000, timeoutMsg: "Truth import did not produce a terminal UI state" },
    );
    assert.equal(truthOutcome.ready, true, truthOutcome.error);

    const storeRoot = path.join(appDataRoot, "truth", "sts2");
    const current = JSON.parse(await fs.readFile(path.join(storeRoot, "current.json"), "utf8"));
    await fs.access(path.join(storeRoot, "snapshots", current.snapshotId, "truth-snapshot.json"));
    assert.deepEqual(await fs.readdir(path.join(storeRoot, ".staging")), []);
    assert.ok(!storeRoot.includes(path.join("projects", "E2EMod")));
  });

  it("drives staged Composition pause, resume, and cancel through one persisted graph", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");
    const draftId = "gui-staged-controls";

    await navigate("/composition");
    await waitForTestId("composition-profile", 60_000);
    await selectValue("composition-profile", "prototype");
    await $('[data-testid="composition-draft-id"]').setValue(draftId);
    await $('[data-testid="composition-concept"]').setValue("GUI staged control-state E2E");
    await $('[data-testid="composition-plan"]').click();

    const running = await waitForCompositionGraph("running");
    await waitForEnabled("composition-execution-pause");
    await $('[data-testid="composition-execution-pause"]').click();
    const paused = await waitForCompositionGraph("paused", { graphId: running.graphId });
    assert.equal(paused.graphId, running.graphId);
    await assert.rejects(() => fs.access(path.join(
      projectRoot,
      ".ats",
      "composition-drafts-v2",
      `${draftId}.json`,
    )));

    await waitForEnabled("composition-execution-resume");
    await $('[data-testid="composition-execution-resume"]').click();
    const resumed = await waitForCompositionGraph("running", {
      graphId: running.graphId,
      previousRunId: running.runId,
    });
    await waitForEnabled("composition-execution-cancel");
    await $('[data-testid="composition-execution-cancel"]').click();
    const cancelled = await waitForCompositionGraph("cancelled", {
      graphId: running.graphId,
      previousRunId: running.runId,
    });
    assert.equal(cancelled.runId, resumed.runId);

    const graph = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "execution-graphs-v4",
      `${running.graphId}.json`,
    ), "utf8"));
    assert.equal(graph.schemaVersion, 4);
    assert.equal(graph.status, "cancelled");
    assert.equal(graph.activeRunId, null);
    assert.ok(Object.values(graph.nodes).every((node) => node.status !== "running"));
    await assertNoTransactionResidue(projectRoot);
    await assertNoAtomicWriteResidue(projectRoot);
  });

  it("resumes only the invalid staged node and atomically commits Draft v2 provenance", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");
    const draftId = "gui-staged-recovery";

    await navigate("/composition");
    await waitForTestId("composition-profile", 60_000);
    await selectValue("composition-profile", "prototype");
    await $('[data-testid="composition-draft-id"]').setValue(draftId);
    await $('[data-testid="composition-concept"]').setValue("GUI staged recovery E2E");
    await $('[data-testid="composition-plan"]').click();

    const paused = await waitForCompositionGraph("paused");
    assert.equal(paused.totalNodes, 15);
    assert.equal(paused.completedNodes, 11);
    const graphPath = path.join(
      projectRoot,
      ".ats",
      "execution-graphs-v4",
      `${paused.graphId}.json`,
    );
    const pausedGraph = JSON.parse(await fs.readFile(graphPath, "utf8"));
    assert.equal(pausedGraph.schemaVersion, 4);
    assert.equal(pausedGraph.status, "paused");
    assert.equal(pausedGraph.activeRunId, null);
    const firstRun = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "runs-v3",
      `${paused.runId}.json`,
    ), "utf8"));
    assert.equal(firstRun.status, "failed");
    assert.equal(firstRun.failure.code, "model.output_invalid");
    await assert.rejects(() => fs.access(path.join(
      projectRoot,
      ".ats",
      "composition-drafts-v2",
      `${draftId}.json`,
    )));

    const beforeResume = (await readStubRequests(root)).filter(
      (entry) => entry.scenario === "recovery",
    );
    assert.equal(beforeResume.filter((entry) => entry.kind === "composition_suite_brief").length, 1);
    const initialNodeRequests = beforeResume.filter((entry) => entry.kind === "composition_node");
    assert.equal(initialNodeRequests.length, 11);
    assert.equal(initialNodeRequests.filter((entry) => entry.outcome === "invalid").length, 1);

    await waitForEnabled("composition-execution-resume");
    await $('[data-testid="composition-execution-resume"]').click();
    const succeeded = await waitForCompositionGraph("succeeded", {
      graphId: paused.graphId,
      previousRunId: paused.runId,
      runStatus: "succeeded",
    });
    assert.equal(succeeded.completedNodes, 15);
    assert.equal(succeeded.totalNodes, 15);
    assert.equal(succeeded.runStatus, "succeeded");

    const committedGraph = JSON.parse(await fs.readFile(graphPath, "utf8"));
    assert.equal(committedGraph.status, "succeeded");
    assert.equal(committedGraph.activeRunId, null);
    assert.ok(committedGraph.finalResultRef);
    assert.ok(Object.values(committedGraph.nodes).every((node) => node.status === "succeeded"));
    const draft = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "composition-drafts-v2",
      `${draftId}.json`,
    ), "utf8"));
    assert.equal(draft.schemaVersion, 2);
    assert.equal(draft.sourceExecutionGraphId, paused.graphId);
    assert.match(draft.validatedContentDigest, /^[a-f0-9]{64}$/);
    assert.equal(Object.keys(draft.nodes).length, 11);
    const resumedRun = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "runs-v3",
      `${succeeded.runId}.json`,
    ), "utf8"));
    assert.equal(resumedRun.status, "succeeded");

    const afterResume = (await readStubRequests(root)).filter(
      (entry) => entry.scenario === "recovery",
    );
    assert.equal(afterResume.filter((entry) => entry.kind === "composition_suite_brief").length, 1);
    const allNodeRequests = afterResume.filter((entry) => entry.kind === "composition_node");
    assert.equal(allNodeRequests.length, 12);
    const countsByNode = new Map();
    for (const entry of allNodeRequests) {
      countsByNode.set(entry.nodeId, (countsByNode.get(entry.nodeId) ?? 0) + 1);
    }
    assert.equal(Array.from(countsByNode.values()).filter((count) => count === 2).length, 1);
    assert.equal(Array.from(countsByNode.values()).filter((count) => count === 1).length, 10);
    await assertNoTransactionResidue(projectRoot);
    await assertNoAtomicWriteResidue(projectRoot);
  });

  it("feeds back one invalid Generate Single and publishes one complete closure", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");
    const draftId = "gui-staged-recovery";
    const artifactId = "gui-composition";

    await navigate("/composition");
    await waitForTestId(`composition-draft-${draftId}`, 60_000);
    await $(`[data-testid="composition-draft-${draftId}"]`).click();
    const draft = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "composition-drafts-v2",
      `${draftId}.json`,
    ), "utf8"));
    const cards = Object.values(draft.nodes)
      .filter((node) => node.definition.itemType === "card")
      .map((node) => node.definition.itemId);
    const relics = Object.values(draft.nodes)
      .filter((node) => node.definition.itemType === "relic")
      .map((node) => node.definition.itemId);
    assert.equal(cards.length, 9);
    assert.equal(relics.length, 1);
    for (const [index, itemId] of cards.entries()) {
      await $(`[data-testid="composition-node-${itemId}"]`).click();
      if (index === 0) {
        await waitForTestId("resource-ai-prompt", 60_000);
        await $('[data-testid="resource-ai-prompt"]').setValue("Deterministic card identity art");
        await waitForEnabled("resource-ai-card.master", 60_000);
        await $('[data-testid="resource-ai-card.master"]').click();
      }
      await bindRequiredResourceCandidates(2);
    }
    for (const [index, itemId] of relics.entries()) {
      await $(`[data-testid="composition-node-${itemId}"]`).click();
      if (index === 0) {
        await waitForTestId("resource-ai-prompt", 60_000);
        await $('[data-testid="resource-ai-prompt"]').setValue("Deterministic relic identity art");
        await waitForEnabled("resource-ai-relic.master", 60_000);
        await $('[data-testid="resource-ai-relic.master"]').click();
      }
      await bindRequiredResourceCandidates(3);
    }
    await browser.execute(() => {
      const button = Array.from(document.querySelectorAll("button"))
        .find((candidate) => candidate.textContent?.includes("Select filtered"));
      if (!(button instanceof HTMLButtonElement)) throw new Error("Select filtered button is missing");
      button.click();
    });
    await waitForEnabled("composition-confirm");
    await $('[data-testid="composition-confirm"]').click();
    await waitForTestId("composition-generate-root", 60_000);
    await browser.waitUntil(
      async () => browser.execute(() => {
        const element = document.querySelector('[data-testid="composition-generate-root"]');
        return element instanceof HTMLSelectElement && element.options.length > 0 && Boolean(element.value);
      }),
      { timeout: 60_000, timeoutMsg: "confirmed composition root did not become available" },
    );
    await $('[data-testid="composition-artifact-id"]').setValue(artifactId);
    await $('[data-testid="composition-mod-id"]').setValue("E2EMod");
    await $('[data-testid="composition-source-root"]').setValue("delivery");
    await $('[data-testid="composition-output-path"]').setValue("packages/E2EMod-composition.zip");
    await $('[data-testid="composition-generate"]').click();

    const running = await waitForCompositionGraph("running", { timeout: 60_000 });
    const succeeded = await waitForCompositionGraph("succeeded", {
      graphId: running.graphId,
      runStatus: "succeeded",
      timeout: 360_000,
    });
    assert.equal(succeeded.runId, running.runId);
    assert.equal(succeeded.completedNodes, 23);
    assert.equal(succeeded.totalNodes, 23);
    const graphPath = path.join(
      projectRoot,
      ".ats",
      "execution-graphs-v4",
      `${succeeded.graphId}.json`,
    );
    const committedGraph = JSON.parse(await fs.readFile(graphPath, "utf8"));
    assert.equal(committedGraph.schemaVersion, 4);
    assert.equal(committedGraph.status, "succeeded");
    assert.equal(committedGraph.activeRunId, null);
    assert.ok(Object.values(committedGraph.nodes).every((node) => node.status === "succeeded"));
    const feedbackNodes = Object.values(committedGraph.nodes).filter(
      (node) => node.feedbackState,
    );
    assert.equal(feedbackNodes.length, 1);
    assert.equal(feedbackNodes[0].roleId, "mod.generate.single");
    assert.equal(feedbackNodes[0].attemptCount, 1);
    assert.equal(feedbackNodes[0].feedbackState.round, 1);
    assert.equal(feedbackNodes[0].feedbackState.phase, "output_contract");
    assert.match(feedbackNodes[0].feedbackState.candidateSha256, /^[a-f0-9]{64}$/);
    assert.equal(feedbackNodes[0].feedbackState.checkpointHash, undefined);
    assert.deepEqual(feedbackNodes[0].feedbackState.feedback.payload.schema, {
      id: "feature.generation-feedback",
      version: 2,
    });
    const succeededRun = JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "runs-v3",
      `${succeeded.runId}.json`,
    ), "utf8"));
    assert.equal(succeededRun.status, "succeeded");
    const payload = succeededRun.result.payload;
    assert.equal(payload.executionGraphId, succeeded.graphId);
    assert.equal(payload.nodeCount, 11);
    assert.equal(payload.items.length, 11);
    const manifestPath = path.join(projectRoot, payload.artifactManifestRef);
    assert.equal(await sha256File(manifestPath), payload.manifestSha256);
    const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8"));
    assert.equal(manifest.producingRunId, succeeded.runId);
    assert.equal(manifest.featureExtension.payload.executionGraphId, succeeded.graphId);
    const artifactRuns = await fs.readdir(path.join(projectRoot, "artifacts", artifactId, "runs"));
    assert.deepEqual(artifactRuns, [succeeded.runId]);
    await fs.access(path.join(projectRoot, payload.package.outputRelativePath));

    const generateRequests = (await readStubRequests(root)).filter(
      (entry) => entry.kind === "composition_generate_single",
    );
    assert.equal(generateRequests.length, 12);
    assert.equal(generateRequests.filter((entry) => entry.outcome === "invalid").length, 1);
    const singleCounts = new Map();
    for (const entry of generateRequests) {
      singleCounts.set(entry.itemId, (singleCounts.get(entry.itemId) ?? 0) + 1);
    }
    assert.equal(Array.from(singleCounts.values()).filter((count) => count === 2).length, 1);
    assert.equal(Array.from(singleCounts.values()).filter((count) => count === 1).length, 10);
    const singleRuns = await Promise.all((await fs.readdir(path.join(
      projectRoot,
      ".ats",
      "runs-v3",
    ))).map(async (file) => JSON.parse(await fs.readFile(path.join(
      projectRoot,
      ".ats",
      "runs-v3",
      file,
    ), "utf8"))));
    const failedSingleRuns = singleRuns.filter(
      (run) => run.featureId === "mod.generate.single" && run.status === "failed",
    );
    assert.equal(failedSingleRuns.length, 1);
    assert.equal(failedSingleRuns[0].failure.code, "model.output_invalid");
    const generatePlans = (await readStubRequests(root)).filter(
      (entry) => entry.kind === "composition_generate_plan",
    );
    assert.equal(generatePlans.length, 11);
    assert.deepEqual(
      Object.fromEntries(["character", "card", "relic"].map((itemType) => [
        itemType,
        generatePlans.filter((entry) => entry.itemType === itemType).length,
      ])),
      { character: 1, card: 9, relic: 1 },
    );
    assert.deepEqual(await fs.readdir(path.join(projectRoot, ".ats", "composition-staging")).catch(() => []), []);
    await assertNoTransactionResidue(projectRoot);
    await assertNoAtomicWriteResidue(projectRoot);
  });

  it("saves a definition and executes Batch v4 through persisted child Runs", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");

    await navigate("/editor");
    await waitForTestId("new-item-type", 60_000);
    await selectValue("new-item-type", "custom_code");
    await $('[data-testid="new-item-id"]').setValue("gui-code");
    await $('[data-testid="new-item-submit"]').click();
    await waitForTestId("item-behavior-intent");
    await $('[data-testid="item-behavior-intent"]').setValue("Expose one compile-test fixture type.");
    await $('[data-testid="item-save"]').click();
    await waitForTestId("item-saved-hash", 30_000);
    const definitionHash = (await $('[data-testid="item-saved-hash"]').getText()).trim();
    assert.match(definitionHash, /^[a-f0-9]{64}$/);

    await navigate("/batch");
    await waitForTestId("batch-definition", 30_000);
    const checkbox = await $('[data-testid="batch-definition"][data-item-id="gui-code"] input[type="checkbox"]');
    await checkbox.click();
    const request = JSON.parse(await $('[data-testid="batch-request-preview"]').getText());
    assert.equal(request.items[0].definition.definitionHash, definitionHash);
    assert.equal(request.items[0].definition.definition.itemType, "custom_code");

    await $('[data-testid="batch-submit"]').click();
    const terminal = await waitForBatchRun("succeeded");
    const payload = JSON.parse(await $('[data-testid="batch-run-payload"]').getText());
    assert.deepEqual(
      { total: payload.total, processed: payload.processed, succeeded: payload.succeeded, failed: payload.failed },
      { total: 1, processed: 1, succeeded: 1, failed: 0 },
    );
    assert.equal(payload.items[0].definitionHash, definitionHash);
    assert.equal(payload.items[0].status, "succeeded");
    assert.ok(payload.items[0].planRunId);
    assert.ok(payload.items[0].generationRunId);

    const children = await $$('[data-testid="batch-child-run"]');
    assert.equal(children.length, 2);
    for (const child of children) assert.equal(await child.getAttribute("data-run-status"), "succeeded");

    const manifestPath = path.join(projectRoot, payload.items[0].result.artifactManifestRef);
    assert.equal(await sha256File(manifestPath), payload.items[0].result.manifestSha256);
    const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8"));
    assert.equal(manifest.producingRunId, payload.items[0].generationRunId);
    assert.equal(manifest.featureExtension.payload.definitionHash, definitionHash);
    assert.ok(terminal.id);
    const runsRoot = path.dirname(path.dirname(manifestPath));
    assert.ok(!(await fs.readdir(runsRoot)).some((name) => name.startsWith(".staging-")));
    await assertNoTransactionResidue(projectRoot);
  });

  it("executes Complex v3 and packages only after Batch succeeds", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");
    const localPropsPath = path.join(projectRoot, "local.props");
    const localProps = await fs.readFile(localPropsPath, "utf8");
    assert.match(localProps, /<ModsPath>[^<]+<\/ModsPath>/);
    await fs.mkdir(path.join(projectRoot, "delivery"), { recursive: true });
    await fs.mkdir(path.join(projectRoot, "packages"), { recursive: true });

    await navigate("/batch");
    await waitForTestId("batch-definition", 30_000);
    await $('[data-testid="batch-mode-complex"]').click();
    const checkbox = await $('[data-testid="batch-definition"][data-item-id="gui-code"] input[type="checkbox"]');
    if (!(await checkbox.isSelected())) await checkbox.click();
    const request = JSON.parse(await $('[data-testid="batch-request-preview"]').getText());
    assert.equal(request.batch.items.length, 1);
    assert.equal(request.package.sourceRelativeRoot, "delivery");

    const previousRunId = await $('[data-testid="batch-run-result"]').getAttribute("data-run-id");
    await $('[data-testid="batch-submit"]').click();
    await waitForBatchRun("succeeded", 300_000, previousRunId);
    const payload = JSON.parse(await $('[data-testid="batch-run-payload"]').getText());
    assert.equal(payload.batch.failed, 0);
    assert.ok(payload.buildRunId);
    assert.ok(payload.packageRunId);
    assert.equal(payload.package.outputRelativePath, "packages/E2EMod.zip");
    await fs.access(path.join(projectRoot, payload.package.outputRelativePath));

    const children = await $$('[data-testid="batch-child-run"]');
    assert.equal(children.length, 5);
    for (const child of children) assert.equal(await child.getAttribute("data-run-status"), "succeeded");
    await assertNoTransactionResidue(projectRoot);
  });

  it("persists an explicit global Godot clear without overriding project-local Build input", async () => {
    const configPath = requiredEnv("SPIREFORGE_CONFIG_PATH");
    const root = requiredEnv("ATS_E2E_ROOT");

    await navigate("/system");
    await waitForTestId("godot-exe-path");
    await $('[data-testid="godot-exe-path"]').setValue("");
    await $('[data-testid="settings-save"]').click();
    await waitForJsonFile(configPath, (value) => value.toolchain?.godot_exe_path === "");

    await navigate("/batch");
    await waitForTestId("batch-mode-build");
    await $('[data-testid="batch-mode-build"]').click();
    await $('[data-testid="batch-submit"]').click();
    await waitForBatchRun("succeeded", 180_000);
    const payload = JSON.parse(await $('[data-testid="batch-run-payload"]').getText());
    assert.equal(payload.steps.length, 1);
    const localProps = await fs.readFile(path.join(root, "projects", "E2EMod", "local.props"), "utf8");
    assert.match(localProps, /<GodotPath>[^<]+<\/GodotPath>/);
  });
});
