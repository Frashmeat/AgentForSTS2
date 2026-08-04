import path from "node:path";

const appBinary = path.resolve(
  process.env.ATS_E2E_APP_BINARY ?? "target/debug/agentthespire-desktop.exe",
);

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.mjs"],
  maxInstances: 1,
  services: [["@wdio/tauri-service", {
    appBinaryPath: appBinary,
    driverProvider: "embedded",
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
