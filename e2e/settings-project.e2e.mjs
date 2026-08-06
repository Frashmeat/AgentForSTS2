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
