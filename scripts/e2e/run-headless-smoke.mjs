import fs from "node:fs";
import path from "node:path";

import { HeadlessClient } from "./headless-client.mjs";

const binary = path.resolve(requiredEnv("ATS_HEADLESS_APP_BINARY"));
if (!fs.statSync(binary, { throwIfNoEntry: false })?.isFile()) {
  throw new Error(`ATS_HEADLESS_APP_BINARY is not a file: ${binary}`);
}
requiredEnv("SPIREFORGE_APP_DATA_ROOT");

const client = HeadlessClient.start({ binary });
try {
  const health = await client.request("health");
  const settings = await client.request("get_settings");
  const project = await client.request("current_project");
  assertExpected("ATS_EXPECTED_BUILD_ID", client.build.buildId);
  assertExpected("ATS_EXPECTED_COMMIT", client.build.commit);
  assertExpected("ATS_EXPECTED_VARIANT", client.build.variant);
  if (health.status !== "ok" || health.featureCount !== 12) {
    throw new Error("headless health contract is not ready");
  }
  if (settings.llm.apiKeyConfigured !== true && settings.llm.apiKeyConfigured !== false) {
    throw new Error("headless settings snapshot did not redact configuration state");
  }
  if (project !== null) {
    throw new Error("headless smoke expected no active project");
  }
  process.stdout.write(`${JSON.stringify({
    status: "ok",
    build: client.build,
    gamePackId: health.gamePackId,
    gamePackSha256: health.gamePackSha256,
    featureCount: health.featureCount,
    truthReady: health.truthReady,
    llmConfigured: settings.llm.apiKeyConfigured,
    imageGenerationConfigured: settings.imageGen.apiKeyConfigured,
  })}\n`);
} finally {
  await client.shutdown();
}

function requiredEnv(name) {
  const value = process.env[name];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function assertExpected(name, actual) {
  const expected = process.env[name];
  if (expected !== undefined && expected !== actual) {
    throw new Error(`${name} mismatch: expected ${expected}, received ${actual}`);
  }
}
