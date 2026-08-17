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

const { buildPatch, formFromSnapshot, validateMaxOutputTokens } = await vite.ssrLoadModule(
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
    customPrompt: "",
    maxOutputTokens: null,
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
    githubTokenMasked: "ghp_...mask (len=40)",
  },
  runtimeWeb: {
    host: "127.0.0.1",
    port: 7870,
    mountFrontend: false,
    requiresDatabase: true,
    githubTokenMasked: "<empty>",
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
    toolchain: { godotExePath: "D:/Tools/Godot/godot.exe" },
  });
});

test("masked GitHub token is not loaded into the editable form", () => {
  const form = formFromSnapshot(snapshot);

  assert.equal(form.githubToken, "");
  assert.equal(form.githubTokenTouched, false);
  assert.deepEqual(buildPatch(form, snapshot), {});
});

test("a new GitHub token is written exactly as entered", () => {
  const form = {
    ...formFromSnapshot(snapshot),
    githubToken: "ghp_new-token",
    githubTokenTouched: true,
  };

  assert.deepEqual(buildPatch(form, snapshot), {
    runtimeWorkstation: { githubToken: "ghp_new-token" },
  });
});

test("runtime custom instructions use the typed LLM patch", () => {
  const form = {
    ...formFromSnapshot(snapshot),
    llmCustomPrompt: "Keep generated names concise.",
  };
  assert.deepEqual(buildPatch(form, snapshot), {
    llm: { customPrompt: "Keep generated names concise." },
  });
});

test("optional model output budget round trips through the LLM patch", () => {
  const blank = formFromSnapshot(snapshot);
  assert.equal(blank.llmMaxOutputTokens, "");
  assert.deepEqual(buildPatch(blank, snapshot), {});

  const configured = { ...blank, llmMaxOutputTokens: "4096" };
  assert.deepEqual(buildPatch(configured, snapshot), {
    llm: { maxOutputTokens: 4096 },
  });

  const configuredSnapshot = {
    ...snapshot,
    llm: { ...snapshot.llm, maxOutputTokens: 8192 },
  };
  const cleared = { ...formFromSnapshot(configuredSnapshot), llmMaxOutputTokens: "" };
  assert.deepEqual(buildPatch(cleared, configuredSnapshot), {
    llm: { maxOutputTokens: null },
  });
});

test("model output budget validation rejects non-integers and invalid bounds", () => {
  assert.equal(validateMaxOutputTokens(""), null);
  assert.equal(validateMaxOutputTokens("1"), null);
  assert.equal(validateMaxOutputTokens("65536"), null);
  for (const invalid of ["0", "-1", "1.5", "65537", "value"]) {
    assert.match(validateMaxOutputTokens(invalid), /whole number/);
  }
});

test("an explicitly cleared GitHub token is preserved as an empty patch value", () => {
  const form = {
    ...formFromSnapshot(snapshot),
    githubToken: "",
    githubTokenTouched: true,
  };

  assert.deepEqual(buildPatch(form, snapshot), {
    runtimeWorkstation: { githubToken: "" },
  });
});
