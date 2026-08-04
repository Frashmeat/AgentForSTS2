import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import process from "node:process";
import { deflateSync } from "node:zlib";

const root = process.env.ATS_E2E_ROOT;
if (!root) throw new Error("ATS_E2E_ROOT is required");
const baseLibPath = process.env.ATS_E2E_BASELIB_PATH;
if (!baseLibPath) throw new Error("ATS_E2E_BASELIB_PATH is required");

const plan = {
  itemId: "gui-code",
  itemType: "custom_code",
  name: "GUI Code",
  summary: "A deterministic custom-code item for GUI verification.",
  behaviorIntent: ["Expose one compile-test fixture type."],
  implementationConstraints: ["Compile as a minimal C# declaration."],
  evidenceRequirements: ["Use the verified custom-code contract."],
  acceptanceCriteria: ["The generated project compiles."],
};

const bundle = {
  files: { source: "public sealed class GuiCode {}" },
  acceptanceNotes: ["The deterministic custom-code bundle was assembled."],
};

const compileFailureBundle = {
  files: { source: "public sealed class CompileFailureCode { private MissingType value; }" },
  acceptanceNotes: ["The deterministic compile failure was assembled."],
};

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])));
  return Buffer.concat([length, typeBytes, data, checksum]);
}

function deterministicSubjectPng() {
  const width = 64;
  const height = 64;
  const scanlines = Buffer.alloc(height * (1 + width * 4));
  for (let y = 0; y < height; y += 1) {
    const row = y * (1 + width * 4);
    scanlines[row] = 0;
    for (let x = 0; x < width; x += 1) {
      const offset = row + 1 + x * 4;
      const subject = x >= 16 && x < 48 && y >= 16 && y < 48;
      scanlines[offset] = subject ? 200 : 255;
      scanlines[offset + 1] = subject ? 40 : 255;
      scanlines[offset + 2] = subject ? 30 : 255;
      scanlines[offset + 3] = 255;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(scanlines)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

const pngBase64 = deterministicSubjectPng().toString("base64");

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function readJson(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

async function record(kind) {
  await fs.appendFile(path.join(root, "stub-requests.jsonl"), `${JSON.stringify({ kind })}\n`);
}

async function completeResponse(response, content, initialDelay) {
  await delay(initialDelay);
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify({
    id: "e2e-stub",
    model: "e2e-stub",
    choices: [{ index: 0, message: { role: "assistant", content }, finish_reason: "stop" }],
    usage: { prompt_tokens: 10, completion_tokens: 10 },
  }));
}

const server = http.createServer(async (request, response) => {
  try {
    if (request.method === "GET" && request.url === "/baselib/releases/latest") {
      const authorization = request.headers.authorization ?? "";
      await record(`baselib_${authorization === "Bearer e2e-401" ? "401" : authorization === "Bearer e2e-403" ? "403" : "release"}`);
      if (authorization === "Bearer e2e-401") {
        response.writeHead(401, { "content-type": "application/json" });
        response.end(JSON.stringify({ message: "Bad credentials", secret: "must-not-leak" }));
      } else if (authorization === "Bearer e2e-403") {
        response.writeHead(403, { "content-type": "application/json" });
        response.end(JSON.stringify({ message: "API rate limit exceeded for 203.0.113.1" }));
      } else {
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify({
          tag_name: "v3.3.8",
          assets: [{
            name: "BaseLib.dll",
            browser_download_url: `http://${request.headers.host}/baselib/assets/BaseLib.dll`,
          }],
        }));
      }
      return;
    }
    if (request.method === "GET" && request.url === "/baselib/assets/BaseLib.dll") {
      await record("baselib_download");
      const bytes = await fs.readFile(baseLibPath);
      response.writeHead(200, {
        "content-type": "application/octet-stream",
        "content-length": bytes.length,
      });
      response.end(bytes);
      return;
    }
    if (request.method === "POST" && request.url === "/v1/chat/completions") {
      const body = await readJson(request);
      const isPlan = body.messages?.some((message) =>
        message.role === "system"
          && String(message.content).includes("Plan exactly one independently testable item"));
      const promptText = body.messages
        ?.map((message) => String(message.content ?? ""))
        .join("\n") ?? "";
      const isCompileFailure = promptText.includes("CompileFailureRelic");
      await record(isPlan ? "plan" : isCompileFailure ? "asset_bundle_compile_failure" : "asset_bundle");
      await completeResponse(
        response,
        JSON.stringify(isPlan ? plan : isCompileFailure ? compileFailureBundle : bundle),
        isPlan ? 1_200 : 100,
      );
      return;
    }
    if (request.method === "POST" && request.url === "/v1/images/generations") {
      await readJson(request);
      await record("image");
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        created: 0,
        data: [{ b64_json: pngBase64, revised_prompt: "deterministic local image" }],
      }));
      return;
    }
    response.writeHead(404, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: { message: "not found" } }));
  } catch (error) {
    response.writeHead(500, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: { message: String(error) } }));
  }
});

server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("stub address unavailable");
  process.stdout.write(`ATS_E2E_STUB_URL=http://127.0.0.1:${address.port}\n`);
});

process.on("SIGTERM", () => server.close(() => process.exit(0)));
process.on("SIGINT", () => server.close(() => process.exit(0)));
