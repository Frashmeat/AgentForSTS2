import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const PROTOCOL_VERSION = 1;
const MAX_DIAGNOSTIC_CHARS = 8_192;

export class HeadlessCommandError extends Error {
  constructor(requestId, failure) {
    super(`${failure.code} at ${failure.stage}`);
    this.name = "HeadlessCommandError";
    this.requestId = requestId;
    this.failure = failure;
  }
}

export class HeadlessClient {
  static start({ binary, env = process.env }) {
    if (typeof binary !== "string" || binary.length === 0) {
      throw new TypeError("binary is required");
    }
    return new HeadlessClient(binary, env);
  }

  #child;
  #nextRequest = 1;
  #pending = new Map();
  #stderr = "";
  #build = null;
  #closed = false;
  #exit;

  constructor(binary, env) {
    this.#child = spawn(binary, ["--headless-jsonl"], {
      env,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.#exit = new Promise((resolve) => {
      this.#child.once("exit", (code, signal) => {
        this.#closed = true;
        const diagnostic = boundedDiagnostic(this.#stderr);
        for (const { reject } of this.#pending.values()) {
          reject(new Error(`headless process exited before responding: code=${code} signal=${signal}${diagnostic}`));
        }
        this.#pending.clear();
        resolve({ code, signal });
      });
    });
    this.#child.once("error", (error) => {
      for (const { reject } of this.#pending.values()) reject(error);
      this.#pending.clear();
    });
    this.#child.stderr.setEncoding("utf8");
    this.#child.stderr.on("data", (chunk) => {
      this.#stderr = `${this.#stderr}${chunk}`.slice(-MAX_DIAGNOSTIC_CHARS);
    });
    const lines = createInterface({ input: this.#child.stdout, crlfDelay: Infinity });
    lines.on("line", (line) => this.#onResponse(line));
  }

  get build() {
    return this.#build;
  }

  async request(name, input) {
    if (this.#closed || this.#child.stdin.destroyed) {
      throw new Error("headless process is closed");
    }
    const requestId = `driver-${String(this.#nextRequest).padStart(6, "0")}`;
    this.#nextRequest += 1;
    const command = { name };
    if (input !== undefined) command.input = input;
    const request = {
      schemaVersion: PROTOCOL_VERSION,
      requestId,
      command,
    };
    const response = new Promise((resolve, reject) => {
      this.#pending.set(requestId, { resolve, reject });
    });
    this.#child.stdin.write(`${JSON.stringify(request)}\n`, "utf8", (error) => {
      if (!error) return;
      const pending = this.#pending.get(requestId);
      this.#pending.delete(requestId);
      pending?.reject(error);
    });
    return response;
  }

  async waitForRun(runId, { pollIntervalMs = 1_000, timeoutMs = null } = {}) {
    const startedAt = Date.now();
    for (;;) {
      const run = await this.request("get_run", { runId });
      if (["succeeded", "failed", "cancelled"].includes(run.status)) return run;
      if (timeoutMs !== null && Date.now() - startedAt >= timeoutMs) {
        throw new Error(`Run ${runId} did not reach a terminal state within ${timeoutMs}ms`);
      }
      await delay(pollIntervalMs);
    }
  }

  async waitForExecutionGraph(
    executionGraphId,
    { pollIntervalMs = 1_000, timeoutMs = null } = {},
  ) {
    const startedAt = Date.now();
    for (;;) {
      const graph = await this.request("get_execution_graph", { executionGraphId });
      if (["succeeded", "paused", "cancelled"].includes(graph.status)) return graph;
      if (timeoutMs !== null && Date.now() - startedAt >= timeoutMs) {
        throw new Error(
          `Execution Graph ${executionGraphId} did not reach a stable state within ${timeoutMs}ms`,
        );
      }
      await delay(pollIntervalMs);
    }
  }

  async shutdown() {
    if (!this.#closed) {
      await this.request("shutdown");
      this.#child.stdin.end();
    }
    const outcome = await this.#exit;
    if (outcome.code !== 0) {
      throw new Error(`headless process exited with code ${outcome.code}${boundedDiagnostic(this.#stderr)}`);
    }
  }

  #onResponse(line) {
    let response;
    try {
      response = JSON.parse(line);
    } catch {
      this.#rejectOldest(new Error("headless process returned invalid JSON"));
      return;
    }
    const pendingEntry = response.requestId === null
      ? this.#pending.entries().next().value
      : (this.#pending.has(response.requestId)
        ? [response.requestId, this.#pending.get(response.requestId)]
        : null);
    if (!pendingEntry) return;
    const [pendingId, pending] = pendingEntry;
    this.#pending.delete(pendingId);
    if (
      response.schemaVersion !== PROTOCOL_VERSION
      || typeof response.ok !== "boolean"
      || !isBuildInfo(response.build)
      || (response.requestId !== null && typeof response.requestId !== "string")
      || (response.ok && response.requestId === null)
      || (response.ok && response.failure !== undefined)
      || (!response.ok && !isActionableFailure(response.failure))
    ) {
      pending.reject(new Error("headless response contract is invalid"));
      return;
    }
    if (this.#build === null) this.#build = response.build;
    else if (JSON.stringify(this.#build) !== JSON.stringify(response.build)) {
      pending.reject(new Error("headless process BuildInfo changed during one session"));
      return;
    }
    if (!response.ok) {
      pending.reject(new HeadlessCommandError(response.requestId, response.failure));
      return;
    }
    pending.resolve(response.result);
  }

  #rejectOldest(error) {
    const oldest = this.#pending.entries().next().value;
    if (!oldest) return;
    const [requestId, pending] = oldest;
    this.#pending.delete(requestId);
    pending.reject(error);
  }
}

function isBuildInfo(value) {
  return value !== null
    && typeof value === "object"
    && typeof value.commit === "string"
    && typeof value.variant === "string"
    && Array.isArray(value.features)
    && value.features.every((feature) => typeof feature === "string")
    && typeof value.buildId === "string";
}

function isActionableFailure(value) {
  return value !== null
    && typeof value === "object"
    && typeof value.code === "string"
    && typeof value.category === "string"
    && typeof value.stage === "string"
    && typeof value.message === "string"
    && typeof value.action === "string"
    && typeof value.retryable === "boolean";
}

function boundedDiagnostic(value) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? "" : `; stderr=${trimmed.slice(-MAX_DIAGNOSTIC_CHARS)}`;
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
