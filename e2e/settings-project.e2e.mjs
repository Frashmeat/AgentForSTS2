import assert from "node:assert/strict";
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

const waitForJob = async (kind, status, timeout = 180_000) => {
  await browser.waitUntil(
    async () => browser.execute((expectedKind, expectedStatus) => Array.from(
      document.querySelectorAll('[data-testid="job-row"]'),
    ).some((row) => row.getAttribute("data-job-kind") === expectedKind
      && row.getAttribute("data-job-status") === expectedStatus), kind, status),
    { timeout, timeoutMsg: `${kind} did not reach ${status}` },
  );
};

const setChecked = async (testId, checked) => {
  const element = await $(`[data-testid="${testId}"]`);
  if ((await element.isSelected()) !== checked) await element.click();
  assert.equal(await element.isSelected(), checked, `${testId} did not change checked state`);
};

const listedJobIds = async (kind) => browser.execute((expectedKind) => Array.from(
  document.querySelectorAll('[data-testid="job-row"]'),
).filter((row) => row.getAttribute("data-job-kind") === expectedKind)
  .map((row) => row.getAttribute("data-job-id"))
  .filter(Boolean), kind);

const waitForNewJob = async (kind, status, previousIds, timeout = 240_000) => {
  let jobId = null;
  await browser.waitUntil(
    async () => browser.execute((expectedKind, expectedStatus, oldIds) => {
      const row = Array.from(document.querySelectorAll('[data-testid="job-row"]')).find(
        (candidate) => candidate.getAttribute("data-job-kind") === expectedKind
          && candidate.getAttribute("data-job-status") === expectedStatus
          && !oldIds.includes(candidate.getAttribute("data-job-id")),
      );
      return row?.getAttribute("data-job-id") ?? null;
    }, kind, status, previousIds).then((id) => {
      jobId = id;
      return Boolean(id);
    }),
    { timeout, timeoutMsg: `new ${kind} job did not reach ${status}` },
  );
  return jobId;
};

const openJobResult = async (jobId) => {
  const clicked = await browser.execute((expectedId) => {
    const row = Array.from(document.querySelectorAll('[data-testid="job-row"]')).find(
      (candidate) => candidate.getAttribute("data-job-id") === expectedId,
    );
    const button = row?.querySelector("button");
    button?.click();
    return Boolean(button);
  }, jobId);
  assert.equal(clicked, true, `job row ${jobId} was not clickable`);
  await browser.waitUntil(
    async () => browser.execute((expectedId) => (
      document.querySelector('[data-testid="job-detail"]')?.getAttribute("data-job-id")
        === expectedId
    ), jobId),
    { timeout: 15_000, timeoutMsg: `job detail ${jobId} did not open` },
  );
  return JSON.parse(await $('[data-testid="job-detail-result"]').getText());
};

const countFilesWithExtension = async (root, extension) => {
  let count = 0;
  const pending = [root];
  while (pending.length > 0) {
    const current = pending.pop();
    const entries = await fs.readdir(current, { withFileTypes: true });
    for (const entry of entries) {
      const entryPath = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(entryPath);
      else if (entry.isFile() && path.extname(entry.name).toLowerCase() === extension) count += 1;
    }
  }
  return count;
};

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
    assert.match(await error.getText(), /not a file/i);

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
    await waitForTestId("job-kind");
    await selectValue("job-kind", "build_project");
    await waitForTestId("job-build-project-root");
    await $('[data-testid="job-build-project-root"]').setValue(projectRoot);
    await $('[data-testid="job-submit"]').click();
    await waitForJob("build_project", "completed");

    const builtModRoot = path.join(modsRoot, "E2EMod");
    await fs.access(path.join(builtModRoot, "E2EMod.dll"));
    await fs.access(path.join(builtModRoot, "E2EMod.pck"));

    await selectValue("job-kind", "package_project");
    await waitForTestId("job-package-source-dir");
    await $('[data-testid="job-package-source-dir"]').setValue(builtModRoot);
    await $('[data-testid="job-package-output-path"]').setValue(packagePath);
    await $('[data-testid="job-submit"]').click();
    await waitForJob("package_project", "completed");
    await fs.access(packagePath);
  });

  it("refreshes the complete knowledge source and exposes actionable BaseLib auth errors", async () => {
    const root = requiredEnv("ATS_E2E_ROOT");
    const configPath = requiredEnv("SPIREFORGE_CONFIG_PATH");
    const sts2DllPath = requiredEnv("ATS_E2E_STS2_DLL_PATH");

    await navigate("/system?tab=ops");
    await waitForTestId("job-kind");
    await selectValue("job-kind", "knowledge_refresh");
    await waitForTestId("job-knowledge-dll-path");
    await $('[data-testid="job-knowledge-dll-path"]').setValue(sts2DllPath);
    await setChecked("job-knowledge-force", true);
    const beforeRefresh = await listedJobIds("knowledge_refresh");
    await $('[data-testid="job-submit"]').click();
    const refreshJobId = await waitForNewJob(
      "knowledge_refresh",
      "completed",
      beforeRefresh,
    );
    const refreshResult = await openJobResult(refreshJobId);
    assert.equal(refreshResult.baselibIncluded, false);
    assert.equal(refreshResult.gameCacheHit, false);
    assert.ok(refreshResult.gameCsFileCount > 2_000);

    const knowledgeRoot = path.join(root, "knowledge");
    const manifest = JSON.parse(await fs.readFile(
      path.join(knowledgeRoot, "knowledge-manifest.json"),
      "utf8",
    ));
    const actualCsFiles = await countFilesWithExtension(path.join(knowledgeRoot, "game"), ".cs");
    assert.equal(actualCsFiles, manifest.game.csFileCount);
    assert.equal(actualCsFiles, refreshResult.gameCsFileCount);
    await fs.access(path.join(
      knowledgeRoot,
      "game",
      "MegaCrit.Sts2.Core.Nodes.Screens.Settings",
      "NSettingsScreen.cs",
    ));

    for (const scenario of [
      {
        token: "e2e-401",
        status: 401,
        expected: /invalid or expired.*runtime\.workstation\.github_token/i,
      },
      {
        token: "e2e-403",
        status: 403,
        expected: /rate limit.*valid GitHub token/i,
      },
    ]) {
      await navigate("/system?tab=config");
      await waitForTestId("github-token");
      await $('[data-testid="github-token"]').setValue(scenario.token);
      await $('[data-testid="settings-save"]').click();
      await browser.waitUntil(async () => {
        const persisted = JSON.parse(await fs.readFile(configPath, "utf8"));
        return persisted.runtime?.workstation?.github_token === scenario.token;
      }, { timeout: 15_000, timeoutMsg: `GitHub token for ${scenario.status} was not persisted` });

      await navigate("/system?tab=ops");
      await waitForTestId("job-kind");
      await selectValue("job-kind", "knowledge_refresh");
      await waitForTestId("job-knowledge-dll-path");
      await $('[data-testid="job-knowledge-dll-path"]').setValue(sts2DllPath);
      await setChecked("job-knowledge-include-baselib", true);
      const previousIds = await listedJobIds("knowledge_refresh");
      await $('[data-testid="job-submit"]').click();
      const jobId = await waitForNewJob("knowledge_refresh", "completed", previousIds);
      const result = await openJobResult(jobId);
      assert.equal(result.gameCacheHit, true);
      assert.equal(result.baselibStatus, "warning");
      assert.match(result.baselibError, new RegExp(`API responded ${scenario.status}`));
      assert.match(result.baselibError, scenario.expected);
      assert.doesNotMatch(result.baselibError, /must-not-leak|Bad credentials|203\.0\.113\.1/i);
    }
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
    await waitForTestId("job-kind");
    await selectValue("job-kind", "build_project");
    await waitForTestId("job-build-project-root");
    await $('[data-testid="job-build-project-root"]').setValue(projectRoot);
    await $('[data-testid="job-submit"]').click();
    await waitForTestId("job-error");
    assert.match(
      await $('[data-testid="job-error"]').getText(),
      /toolchain\.godot_exe_path is not configured/i,
    );
  });
});
