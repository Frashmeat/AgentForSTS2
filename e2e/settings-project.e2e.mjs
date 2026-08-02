import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";

import { browser, $ } from "@wdio/globals";

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
    async () => browser.execute((id) => Boolean(document.querySelector(`[data-testid="${id}"]`)), testId),
    { timeout, timeoutMsg: `${testId} did not appear` },
  );
};

const selectValue = async (testId, value) => {
  // The embedded Tauri driver changes native selects without notifying React.
  const selected = await browser.execute((id, nextValue) => {
    const element = document.querySelector(`[data-testid="${id}"]`);
    if (!(element instanceof HTMLSelectElement)) return null;
    const setter = Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      "value",
    )?.set;
    setter?.call(element, nextValue);
    element.dispatchEvent(new Event("change", { bubbles: true }));
    return element.value;
  }, testId, value);
  assert.equal(selected, value, `${testId} did not select ${value}`);
};

const setChecked = async (testId, checked) => {
  const element = await $(`[data-testid="${testId}"]`);
  if ((await element.isSelected()) !== checked) await element.click();
  assert.equal(await element.isSelected(), checked, `${testId} did not change checked state`);
};

const listedRunIds = async (kind) => browser.execute((expectedKind) => Array.from(
  document.querySelectorAll('[data-testid="run-row"]'),
).filter((row) => row.getAttribute("data-run-kind") === expectedKind)
  .map((row) => row.getAttribute("data-run-id"))
  .filter(Boolean), kind);

const waitForNewRun = async (kind, status, previousIds, timeout = 240_000) => {
  let runId = null;
  await browser.waitUntil(
    async () => browser.execute((expectedKind, expectedStatus, oldIds) => {
      const row = Array.from(document.querySelectorAll('[data-testid="run-row"]')).find(
        (candidate) => candidate.getAttribute("data-run-kind") === expectedKind
          && candidate.getAttribute("data-run-status") === expectedStatus
          && !oldIds.includes(candidate.getAttribute("data-run-id")),
      );
      return row?.getAttribute("data-run-id") ?? null;
    }, kind, status, previousIds).then((id) => {
      runId = id;
      return Boolean(id);
    }),
    { timeout, timeoutMsg: `new ${kind} run did not reach ${status}` },
  );
  return runId;
};

const openRunResult = async (runId) => {
  const clicked = await browser.execute((expectedId) => {
    const row = Array.from(document.querySelectorAll('[data-testid="run-row"]')).find(
      (candidate) => candidate.getAttribute("data-run-id") === expectedId,
    );
    const button = row?.querySelector("button");
    button?.click();
    return Boolean(button);
  }, runId);
  assert.equal(clicked, true, `run row ${runId} was not clickable`);
  await browser.waitUntil(
    async () => browser.execute((expectedId) => (
      document.querySelector('[data-testid="run-detail"]')?.getAttribute("data-run-id")
        === expectedId
    ), runId),
    { timeout: 15_000, timeoutMsg: `run detail ${runId} did not open` },
  );
  return JSON.parse(await $('[data-testid="run-detail-result"]').getText());
};

const sha256File = async (filePath) => createHash("sha256")
  .update(await fs.readFile(filePath))
  .digest("hex");

describe("Godot toolchain settings and project creation", () => {
  it("rejects an invalid Godot path, persists 4.5.1, and synchronizes local.props", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const configPath = requiredEnv("SPIREFORGE_CONFIG_PATH");
    const appDataRoot = requiredEnv("SPIREFORGE_APP_DATA_ROOT");
    const godotPath = requiredEnv("ATS_E2E_GODOT_PATH");
    const projectsRoot = path.join(root, "projects");
    const projectRoot = path.join(projectsRoot, "E2EMod");

    await navigate("/system?tab=config");
    const godotInput = await $('[data-testid="godot-exe-path"]');
    await godotInput.waitForDisplayed();

    await godotInput.setValue(path.join(root, "missing-godot.exe"));
    await $('[data-testid="settings-save"]').click();
    const error = await $('[data-testid="settings-error"]');
    await error.waitForDisplayed();
    assert.match(await error.getText(), /not (?:a file|found)/i);

    const validGodotInput = await $('[data-testid="godot-exe-path"]');
    await validGodotInput.setValue(godotPath);
    assert.equal(await validGodotInput.getValue(), godotPath);
    await $('[data-testid="settings-save"]').click();
    const settingsOutcome = await browser.waitUntil(async () => browser.execute(() => ({
      saved: document.querySelector('[data-testid="settings-saved"]')?.textContent ?? "",
      error: document.querySelector('[data-testid="settings-error"]')?.textContent ?? "",
    })).then((outcome) => outcome.saved || outcome.error ? outcome : false));
    assert.ok(settingsOutcome.saved, `valid Godot path was rejected: ${settingsOutcome.error}`);

    const persisted = JSON.parse(await fs.readFile(configPath, "utf8"));
    assert.equal(persisted.toolchain.godot_exe_path, godotPath);

    await $('[data-testid="project-menu"]').click();
    await $('[data-testid="project-create-open"]').click();
    await $('[data-testid="project-parent-dir"]').setValue(projectsRoot);
    await $('[data-testid="project-name"]').setValue("E2EMod");
    await $('[data-testid="project-create-submit"]').click();
    const projectMenu = await $('[data-testid="project-menu"]');
    await browser.waitUntil(async () => (await projectMenu.getText()).includes("E2EMod"));

    const recents = JSON.parse(await fs.readFile(
      path.join(appDataRoot, "recent_projects.json"),
      "utf8",
    ));
    assert.equal(recents.items[0].path, projectRoot);
    const localProps = await fs.readFile(path.join(projectRoot, "local.props"), "utf8");
    assert.match(localProps, /<GodotPath>[^<]*Godot_v4\.5\.1[^<]*<\/GodotPath>/);
  });

  it("refreshes Truth Snapshot under app-data outside the workspace", async () => {
    const appDataRoot = requiredEnv("SPIREFORGE_APP_DATA_ROOT");

    await navigate("/system?tab=ops");
    await waitForTestId("run-kind");
    await selectValue("run-kind", "truth_snapshot_refresh");
    await setChecked("run-truth-snapshot-force", true);
    const previousIds = await listedRunIds("truth_snapshot_refresh");
    await $('[data-testid="run-submit"]').click();
    const runId = await waitForNewRun("truth_snapshot_refresh", "succeeded", previousIds);
    const result = await openRunResult(runId);
    assert.equal(result.kind, "truth_snapshot_refresh");
    assert.equal(result.gamePackId, "sts2");
    assert.equal(result.sourceCount, 2);
    assert.equal(result.indexCount, 2);

    const storeRoot = path.join(appDataRoot, "game-packs", "sts2");
    const current = JSON.parse(await fs.readFile(path.join(storeRoot, "current.json"), "utf8"));
    assert.equal(current.snapshotId, result.snapshotId);
    await fs.access(path.join(storeRoot, "snapshots", result.snapshotId, "snapshot.json"));
    const stagingEntries = await fs.readdir(path.join(storeRoot, ".staging"));
    assert.deepEqual(stagingEntries, []);
    assert.ok(!storeRoot.includes(path.join("AgentTheSpire", "runtime")));
  });

  it("publishes a deterministic asset and retains a typed compile-failure diagnostic", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");

    await navigate("/system?tab=ops");
    await waitForTestId("run-kind");
    await selectValue("run-kind", "asset_generate");
    await selectValue("run-asset-type", "relic");
    await $('[data-testid="run-asset-name"]').setValue("GuiRelic");
    await $('[data-testid="run-asset-project-root"]').setValue(projectRoot);
    await $('[data-testid="run-asset-description"]').setValue("deterministic success fixture");
    await $('[data-testid="run-asset-image-prompt"]').setValue("");
    const beforeSuccess = await listedRunIds("asset_generate");
    await $('[data-testid="run-submit"]').click();
    const successRunId = await waitForNewRun("asset_generate", "succeeded", beforeSuccess);
    const success = await openRunResult(successRunId);
    const manifestPath = path.join(projectRoot, success.artifactManifestRef);
    assert.equal(await sha256File(manifestPath), success.manifestSha256);
    const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8"));
    assert.equal(manifest.producingRunId, successRunId);
    assert.equal(manifest.gameContext.gamePackId, "sts2");
    const runRoot = path.dirname(manifestPath);
    for (const file of manifest.files) {
      const snapshotPath = path.join(runRoot, file.snapshotRelativePath);
      assert.equal((await fs.stat(snapshotPath)).size, file.byteLength);
      assert.equal(await sha256File(snapshotPath), file.sha256);
    }
    const runsRoot = path.dirname(runRoot);
    assert.ok(!(await fs.readdir(runsRoot)).some((name) => name.startsWith(".staging-")));
    await assert.rejects(fs.access(path.join(projectRoot, ".ats", "diagnostics", successRunId)));

    await $('[data-testid="run-asset-name"]').setValue("CompileFailureRelic");
    await $('[data-testid="run-asset-description"]').setValue("deterministic compile failure fixture");
    const beforeFailure = await listedRunIds("asset_generate");
    await $('[data-testid="run-submit"]').click();
    const failureRunId = await waitForNewRun("asset_generate", "failed", beforeFailure);
    const failed = await openRunResult(failureRunId);
    assert.equal(failed.status, "failed");
    assert.equal(failed.failure.code, "artifact.compile_failed");
    assert.equal(failed.failure.diagnostic.id, failureRunId);

    const diagnostics = path.join(projectRoot, ".ats", "diagnostics", failureRunId);
    await fs.access(path.join(diagnostics, "generated", "Generated", "CompileFailureRelic.cs"));
    const compileReportText = await fs.readFile(path.join(diagnostics, "asset-compile.json"), "utf8");
    const compileReport = JSON.parse(compileReportText);
    assert.equal(compileReport.schemaVersion, 1);
    assert.notEqual(compileReport.exitCode, 0);
    assert.match(`${compileReport.stdoutTail}\n${compileReport.stderrTail}`, /MissingType|CS0246/);
    assert.doesNotMatch(compileReportText, /[A-Z]:\\Users\\/i);
    await assert.rejects(fs.access(path.join(projectRoot, "Generated", "CompileFailureRelic.cs")));
    await assert.rejects(fs.access(path.join(projectRoot, "artifacts", "CompileFailureRelic", "runs", failureRunId)));
    const failureRunsRoot = path.join(projectRoot, "artifacts", "CompileFailureRelic", "runs");
    const failureRunEntries = await fs.readdir(failureRunsRoot).catch(() => []);
    assert.ok(!failureRunEntries.some((name) => name.startsWith(".staging-")));
  });

  it("restores a delayed asset job across routes and completes build/package", async () => {
    requiredEnv("ATS_E2E_STUB_URL");
    const root = requiredEnv("ATS_E2E_ROOT");
    const projectRoot = path.join(root, "projects", "E2EMod");
    const modsRoot = path.join(root, "mods");
    const packagePath = path.join(root, "E2EMod.zip");
    const localPropsPath = path.join(projectRoot, "local.props");
    const localProps = await fs.readFile(localPropsPath, "utf8");
    const isolatedProps = localProps.replace(
      "</PropertyGroup>",
      `  <ModsPath>${modsRoot}${path.sep}</ModsPath>\n  <E2EMarker>preserve-me</E2EMarker>\n</PropertyGroup>`,
    );
    await fs.writeFile(localPropsPath, isolatedProps, "utf8");

    await navigate("/");
    await waitForTestId("single-asset-plan-submit");
    await $('[data-testid="single-asset-requirements"]').setValue("生成一个 GUI E2E 遗物");
    await selectValue("single-asset-type", "relic");
    await $('[data-testid="single-asset-plan-submit"]').click();
    await waitForTestId("single-asset-planning");

    await navigate("/system?tab=ops");
    await browser.pause(250);
    await navigate("/");
    await waitForTestId("single-asset-plan-result", 30_000);

    await $('[data-testid="single-asset-code-submit"]').click();
    await waitForTestId("single-asset-code-result", 180_000);

    const generated = await fs.readFile(path.join(projectRoot, "Generated", "GuiRelic.cs"), "utf8");
    assert.match(generated, /class GuiRelic/);
    const syncedProps = await fs.readFile(localPropsPath, "utf8");
    assert.ok(syncedProps.includes(`<ModsPath>${modsRoot}${path.sep}</ModsPath>`));
    assert.match(syncedProps, /<E2EMarker>preserve-me<\/E2EMarker>/);

    await navigate("/system?tab=ops");
    await waitForTestId("run-kind");
    await selectValue("run-kind", "build_project");
    await waitForTestId("run-build-project-root");
    await $('[data-testid="run-build-project-root"]').setValue(projectRoot);
    const beforeBuild = await listedRunIds("build_project");
    await $('[data-testid="run-submit"]').click();
    await waitForNewRun("build_project", "succeeded", beforeBuild);

    const builtModRoot = path.join(modsRoot, "E2EMod");
    await fs.access(path.join(builtModRoot, "E2EMod.dll"));
    await fs.access(path.join(builtModRoot, "E2EMod.pck"));

    await selectValue("run-kind", "package_project");
    await waitForTestId("run-package-source-dir");
    await $('[data-testid="run-package-source-dir"]').setValue(modsRoot);
    await $('[data-testid="run-package-output-path"]').setValue(packagePath);
    const beforePackage = await listedRunIds("package_project");
    await $('[data-testid="run-submit"]').click();
    await waitForNewRun("package_project", "succeeded", beforePackage);
    await fs.access(packagePath);
  });

  it("persists an explicit Godot clear and blocks build submission", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const configPath = requiredEnv("SPIREFORGE_CONFIG_PATH");
    const projectRoot = path.join(root, "projects", "E2EMod");

    await navigate("/system?tab=config");
    await waitForTestId("godot-exe-path");
    await $('[data-testid="godot-exe-path"]').setValue("");
    await $('[data-testid="settings-save"]').click();
    await waitForTestId("settings-saved");

    const persisted = JSON.parse(await fs.readFile(configPath, "utf8"));
    assert.equal(persisted.toolchain.godot_exe_path, "");

    await navigate("/system?tab=ops");
    await waitForTestId("run-kind");
    await selectValue("run-kind", "build_project");
    await waitForTestId("run-build-project-root");
    await $('[data-testid="run-build-project-root"]').setValue(projectRoot);
    await $('[data-testid="run-submit"]').click();
    await waitForTestId("run-error");
    assert.match(
      await $('[data-testid="run-error"]').getText(),
      /required local toolchain input is not configured/i,
    );
  });
});
