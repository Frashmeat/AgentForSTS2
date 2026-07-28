import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = process.env.ATS_E2E_ROOT;
if (!root) throw new Error("ATS_E2E_ROOT is required");
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const plan = {
  id: "gui_relic",
  type: "relic",
  name: "GuiRelic",
  name_zhs: "界面遗物",
  description: "A deterministic relic for GUI verification.",
  goal: "Verify the desktop delivery chain.",
  detailed_description: "用于本地 GUI E2E。固定输出，不调用外部模型。",
  implementation_notes: "Compile a minimal deterministic C# declaration.",
  needs_image: true,
  image_description: "A simple green crystal relic on a transparent background.",
  depends_on_item_ids: [],
  scope_boundary: "No gameplay behavior beyond compilation.",
  relationship_reason: "Independent verification asset.",
  acceptance_notes: "C#, localization, images, DLL, PCK and zip exist.",
  affected_targets: ["player"],
  relationship_type: "independent",
  clarification_status: "",
  clarification_questions: [],
  provided_image_b64: "",
};

const bundle = {
  csharp: "public sealed class GuiRelic {}",
  localization: {
    eng: {
      "E2EMOD-GUI_RELIC.title": "GUI Relic",
      "E2EMOD-GUI_RELIC.description": "A deterministic verification relic.",
      "E2EMOD-GUI_RELIC.flavor": "Built by the local E2E stub.",
    },
    zhs: {
      "E2EMOD-GUI_RELIC.title": "界面遗物",
      "E2EMOD-GUI_RELIC.description": "用于确定性验证的遗物。",
      "E2EMOD-GUI_RELIC.flavor": "由本地 E2E stub 构建。",
    },
  },
};

const pngBase64 = (await fs.readFile(
  path.join(repoRoot, "src-tauri", "icons", "32x32.png"),
)).toString("base64");

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function readJson(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

async function record(kind) {
  await fs.appendFile(path.join(root, "stub-requests.jsonl"), `${JSON.stringify({ kind })}\n`);
}

async function streamCompletion(response, content, initialDelay) {
  response.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  await delay(initialDelay);
  response.write(`data: ${JSON.stringify({
    id: "e2e-stub",
    model: "e2e-stub",
    choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }],
  })}\n\n`);
  const middle = Math.ceil(content.length / 2);
  for (const text of [content.slice(0, middle), content.slice(middle)]) {
    response.write(`data: ${JSON.stringify({
      id: "e2e-stub",
      model: "e2e-stub",
      choices: [{ index: 0, delta: { content: text }, finish_reason: null }],
    })}\n\n`);
    await delay(50);
  }
  response.write(`data: ${JSON.stringify({
    id: "e2e-stub",
    model: "e2e-stub",
    choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
    usage: { prompt_tokens: 10, completion_tokens: 10 },
  })}\n\n`);
  response.end("data: [DONE]\n\n");
}

const server = http.createServer(async (request, response) => {
  try {
    if (request.method === "POST" && request.url === "/v1/chat/completions") {
      const body = await readJson(request);
      const isPlan = body.messages?.some((message) =>
        message.role === "system" && String(message.content).includes("PlanItem"));
      await record(isPlan ? "plan" : "asset_bundle");
      await streamCompletion(
        response,
        JSON.stringify(isPlan ? plan : bundle),
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
