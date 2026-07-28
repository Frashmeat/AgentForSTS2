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
