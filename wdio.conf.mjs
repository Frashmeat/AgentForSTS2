import path from "node:path";

const appBinary = path.resolve(
  process.env.ATS_E2E_APP_BINARY ?? "target/debug/agentthespire-desktop.exe",
);
const embeddedPort = Number.parseInt(process.env.TAURI_WEBDRIVER_PORT ?? "", 10);

if (!Number.isInteger(embeddedPort) || embeddedPort < 1 || embeddedPort > 65535) {
  throw new Error("TAURI_WEBDRIVER_PORT must be a valid port allocated by the E2E runner");
}

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.mjs"],
  maxInstances: 1,
  services: [["@wdio/tauri-service", {
    appBinaryPath: appBinary,
    driverProvider: "embedded",
    embeddedPort,
    captureBackendLogs: false,
    captureFrontendLogs: false,
    env: {
      SPIREFORGE_CONFIG_PATH: process.env.SPIREFORGE_CONFIG_PATH,
      SPIREFORGE_APP_DATA_ROOT: process.env.SPIREFORGE_APP_DATA_ROOT,
      ATS_E2E_ROOT: process.env.ATS_E2E_ROOT,
      ATS_E2E_GODOT_PATH: process.env.ATS_E2E_GODOT_PATH,
      ATS_E2E_STS2_DLL_PATH: process.env.ATS_E2E_STS2_DLL_PATH,
      ATS_E2E_STUB_URL: process.env.ATS_E2E_STUB_URL,
      ATS_E2E_BASELIB_RELEASE_URL: process.env.ATS_E2E_BASELIB_RELEASE_URL,
      TAURI_WEBDRIVER_PORT: String(embeddedPort),
    },
  }]],
  capabilities: [{
    browserName: "tauri",
    "tauri:options": {
      application: appBinary,
    },
  }],
  logLevel: "error",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 90_000,
  connectionRetryCount: 1,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 300_000,
    bail: true,
  },
};
