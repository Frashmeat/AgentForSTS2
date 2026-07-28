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

const { buildPatch, formFromSnapshot } = await vite.ssrLoadModule(
  "/src/pages/systemForm.ts",
);

const snapshot = {
  configPath: "runtime/agentthespire.config.json",
  configLoaded: true,
  configErrors: [],
  llm: {
    provider: "openai",
    model: "model",
    baseUrl: "https://llm.example/v1",
    apiKeyMasked: "sk-a...-key (len=20)",
    apiKeyConfigured: true,
  },
  imageGen: {
    provider: "openai",
    model: "image-model",
    baseUrl: "https://image.example/v1",
    size: "1024x1024",
    protocol: "images_api",
    apiKeyMasked: "sk-i...-key (len=20)",
    apiKeyConfigured: true,
  },
  runtimeWorkstation: {
    host: "127.0.0.1",
    port: 7860,
    mountFrontend: true,
    requiresDatabase: false,
    githubToken: "ghp_...mask (len=40)",
  },
  runtimeWeb: {
    host: "127.0.0.1",
    port: 7870,
    mountFrontend: false,
    requiresDatabase: true,
    githubToken: "<empty>",
  },
  knowledge: { sts2DllPath: "C:/game/sts2.dll" },
  toolchain: { godotExePath: "I:/Godot/godot.exe" },
};

test("Godot path is loaded and emitted only when changed", () => {
  const unchanged = formFromSnapshot(snapshot);
  assert.equal(unchanged.godotExePath, "I:/Godot/godot.exe");
  assert.deepEqual(buildPatch(unchanged, snapshot), {});

  const changed = { ...unchanged, godotExePath: "D:/Tools/Godot/godot.exe" };
  assert.deepEqual(buildPatch(changed, snapshot), {
    toolchain: { godot_exe_path: "D:/Tools/Godot/godot.exe" },
  });
});

test("masked GitHub token is not loaded into the editable form", () => {
  const form = formFromSnapshot(snapshot);

  assert.equal(form.rtGithubToken, "");
  assert.equal(form.rtGithubTokenTouched, false);
  assert.deepEqual(buildPatch(form, snapshot), {});
});

test("a new GitHub token is written exactly as entered", () => {
  const form = {
    ...formFromSnapshot(snapshot),
    rtGithubToken: "ghp_new-token",
    rtGithubTokenTouched: true,
  };

  assert.deepEqual(buildPatch(form, snapshot), {
    runtime_workstation: { github_token: "ghp_new-token" },
  });
});

test("an explicitly cleared GitHub token is preserved as an empty patch value", () => {
  const form = {
    ...formFromSnapshot(snapshot),
    rtGithubToken: "",
    rtGithubTokenTouched: true,
  };

  assert.deepEqual(buildPatch(form, snapshot), {
    runtime_workstation: { github_token: "" },
  });
});
