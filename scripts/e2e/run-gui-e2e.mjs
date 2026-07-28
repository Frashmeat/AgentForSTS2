import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import fsp from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function resolveExistingFile(value, label) {
  const resolved = path.resolve(value);
  if (!fs.statSync(resolved, { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`${label} is not a file: ${resolved}`);
  }
  return resolved;
}

function readLocalConfig() {
  const localConfigPath = path.join(repoRoot, "runtime", "agentthespire.config.json");
  if (!fs.statSync(localConfigPath, { throwIfNoEntry: false })?.isFile()) return {};
  return JSON.parse(fs.readFileSync(localConfigPath, "utf8"));
}

function configuredPath(envName, fallback, configKey) {
  const value = process.env[envName] ?? fallback;
  if (!value) {
    throw new Error(`${envName} is required or ${configKey} must be set in runtime/agentthespire.config.json`);
  }
  return resolveExistingFile(value, envName);
}

function assertInsideRoot(root, candidate, label) {
  const relative = path.relative(root, candidate);
  if (relative.startsWith("..") || path.isAbsolute(relative)) {
    throw new Error(`${label} must stay inside the E2E root: ${candidate}`);
  }
}

function runNpm(script, env) {
  const command = process.platform === "win32"
    ? (process.env.ComSpec ?? "cmd.exe")
    : "npm";
  const args = process.platform === "win32"
    ? ["/d", "/s", "/c", `npm run ${script}`]
    : ["run", script];
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    const detail = result.error ? `: ${result.error.message}` : "";
    throw new Error(`npm run ${script} failed with exit code ${result.status}${detail}`);
  }
}

function startOpenAiStub(e2eRoot) {
  const stubPath = path.join(repoRoot, "scripts", "e2e", "openai-stub.mjs");
  return new Promise((resolve, reject) => {
    const stub = spawn(process.execPath, [stubPath], {
      cwd: repoRoot,
      env: { ...process.env, ATS_E2E_ROOT: e2eRoot },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      stub.kill();
      reject(new Error(`OpenAI stub did not start: ${stderr}`));
    }, 10_000);
    stub.stdout.setEncoding("utf8");
    stub.stderr.setEncoding("utf8");
    stub.stderr.on("data", (chunk) => { stderr += chunk; });
    stub.stdout.on("data", (chunk) => {
      stdout += chunk;
      const match = stdout.match(/ATS_E2E_STUB_URL=(http:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve({ process: stub, url: match[1] });
      }
    });
    stub.once("exit", (code) => {
      clearTimeout(timer);
      if (!stdout.includes("ATS_E2E_STUB_URL=")) {
        reject(new Error(`OpenAI stub exited with ${code}: ${stderr}`));
      }
    });
  });
}

const localConfig = readLocalConfig();
const godotPath = configuredPath(
  "ATS_E2E_GODOT_PATH",
  localConfig.toolchain?.godot_exe_path,
  "toolchain.godot_exe_path",
);
const sts2Path = configuredPath(
  "ATS_E2E_STS2_DLL_PATH",
  localConfig.knowledge?.sts2_dll_path,
  "knowledge.sts2_dll_path",
);
const e2eRoot = await fsp.mkdtemp(path.join(os.tmpdir(), "ats-gui-e2e-"));
const configPath = path.join(e2eRoot, "config.json");
const appDataRoot = path.join(e2eRoot, "app-data");
const projectsRoot = path.join(e2eRoot, "projects");
const modsRoot = path.join(e2eRoot, "mods");

for (const [label, candidate] of [
  ["config", configPath],
  ["app data", appDataRoot],
  ["projects", projectsRoot],
  ["mods", modsRoot],
]) {
  assertInsideRoot(e2eRoot, candidate, label);
}

await fsp.mkdir(appDataRoot, { recursive: true });
await fsp.mkdir(projectsRoot, { recursive: true });
await fsp.mkdir(modsRoot, { recursive: true });

let passed = false;
let stubProcess;
try {
  const stub = await startOpenAiStub(e2eRoot);
  stubProcess = stub.process;
  await fsp.writeFile(
    configPath,
    `${JSON.stringify({
      llm: {
        provider: "openai",
        model: "e2e-stub",
        api_key: "e2e-local-only",
        base_url: stub.url,
      },
      image_gen: {
        provider: "openai",
        protocol: "images_api",
        model: "e2e-image-stub",
        api_key: "e2e-local-only",
        base_url: stub.url,
        size: "1024x1024",
      },
      knowledge: { sts2_dll_path: sts2Path },
      toolchain: { godot_exe_path: "" },
    }, null, 2)}\n`,
    "utf8",
  );
  const env = {
    ...process.env,
    ATS_E2E_ROOT: e2eRoot,
    ATS_E2E_GODOT_PATH: godotPath,
    ATS_E2E_STS2_DLL_PATH: sts2Path,
    ATS_E2E_STUB_URL: stub.url,
    ATS_E2E_BASELIB_RELEASE_URL: `${stub.url}/baselib/releases/latest`,
    SPIREFORGE_CONFIG_PATH: configPath,
    SPIREFORGE_APP_DATA_ROOT: appDataRoot,
  };
  runNpm("e2e:build", env);
  runNpm("test:e2e:gui:wdio", env);
  passed = true;
} finally {
  stubProcess?.kill();
  if (passed && process.env.ATS_E2E_KEEP_ROOT !== "1") {
    await fsp.rm(e2eRoot, { recursive: true, force: true });
  } else {
    console.error(`GUI E2E evidence retained at: ${e2eRoot}`);
  }
}
