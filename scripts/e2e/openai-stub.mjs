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
  if (response.destroyed || response.writableEnded) return;
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify({
    id: "e2e-stub",
    model: "e2e-stub",
    choices: [{ index: 0, message: { role: "assistant", content }, finish_reason: "stop" }],
    usage: { prompt_tokens: 10, completion_tokens: 10 },
  }));
}

const stagedFailures = new Set();

function taggedJson(promptText, tag) {
  const match = promptText.match(new RegExp(`<${tag}>\\s*([\\s\\S]*?)\\s*</${tag}>`));
  if (!match) throw new Error(`${tag} was not present in the staged prompt`);
  return JSON.parse(match[1]);
}

function stagedScenario(promptText) {
  if (promptText.includes("GUI staged control-state E2E")) return "controls";
  if (promptText.includes("GUI staged recovery E2E")) return "recovery";
  return "default";
}

async function recordStaged(kind, scenario, details = {}) {
  await fs.appendFile(
    path.join(root, "stub-requests.jsonl"),
    `${JSON.stringify({ kind, scenario, ...details })}\n`,
  );
}

function localizedNameDescription(name) {
  return {
    eng: { name, description: `${name} deterministic E2E description.` },
    zhs: { name, description: `${name} deterministic E2E description.` },
  };
}

function characterLocalizations() {
  const fields = {
    title: "Queue Adept",
    title_object: "Queue Adept",
    description: "A deterministic staged-composition Character.",
    pronoun_object: "them",
    pronoun_subject: "they",
    pronoun_possessive: "theirs",
    possessive_adjective: "their",
    aroma_principle: "Resolve one bounded step at a time.",
    end_turn_ping_alive: "The queue advances.",
    end_turn_ping_dead: "The queue is still.",
    event_death_prevention: "Resume from the last checkpoint.",
    gold_monologue: "Every coin has an owner.",
    cards_modifier_title: "Ordered Draw",
    cards_modifier_description: "Cards retain their deterministic order.",
  };
  return { eng: fields, zhs: fields };
}

function suiteBrief(blueprint) {
  const nodeResponsibilities = Object.fromEntries(
    blueprint.itemNodes.map((node) => [
      node.executionNodeId,
      `Provide the bounded ${node.groupId} item at ordinal ${node.ordinal}.`,
    ]),
  );
  const quantityDistributions = {};
  for (const rule of blueprint.bindingRules) {
    if (rule.quantityPolicy?.kind !== "brief_distribution") continue;
    const targets = blueprint.itemNodes.filter((node) =>
      rule.targetGroupIds.includes(node.groupId));
    const total = blueprint.profile.parameters[rule.quantityPolicy.totalParameterId];
    const base = Math.floor(total / targets.length);
    const remainder = total % targets.length;
    quantityDistributions[`${rule.sourceGroupId}.${rule.slotId}`] = Object.fromEntries(
      targets.map((node, index) => [node.executionNodeId, base + (index < remainder ? 1 : 0)]),
    );
  }
  return {
    theme: "A deterministic queue-driven Character suite.",
    nodeResponsibilities,
    quantityDistributions,
  };
}

function stagedNodeCheckpoint(identity) {
  const ordinal = identity.ordinal + 1;
  const name = `${identity.groupId.replaceAll("_", " ")} ${ordinal}`;
  if (identity.itemType === "character") {
    return {
      canonicalFields: {
        visual_profile: { kind: "choice", value: "branded_placeholder" },
        placeholder_id: { kind: "choice", value: "ironclad" },
        name_color: { kind: "text", value: "7D3FC8FF" },
        gender: { kind: "choice", value: "neutral" },
        starting_hp: { kind: "integer", value: 70 },
        starting_gold: { kind: "integer", value: 99 },
        max_energy: { kind: "integer", value: 3 },
      },
      behaviorIntent: ["Provide a playable deterministic Character with locally bound pools."],
      localizations: characterLocalizations(),
    };
  }
  if (identity.itemType === "card") {
    const rarity = {
      starter_cards: "basic",
      common_cards: "common",
      uncommon_cards: "uncommon",
      rare_cards: "rare",
    }[identity.groupId];
    return {
      canonicalFields: {
        pool: { kind: "choice", value: "custom_character" },
        card_type: { kind: "choice", value: ordinal % 2 === 0 ? "skill" : "attack" },
        rarity: { kind: "choice", value: rarity },
        target: { kind: "choice", value: ordinal % 2 === 0 ? "self" : "any_enemy" },
        base_cost: { kind: "integer", value: 1 },
      },
      behaviorIntent: [`Provide deterministic ${identity.groupId} card ${ordinal}.`],
      localizations: localizedNameDescription(name),
    };
  }
  if (identity.itemType === "relic") {
    return {
      canonicalFields: {
        rarity: { kind: "choice", value: identity.groupId === "starter_relics" ? "starter" : "common" },
      },
      behaviorIntent: [`Provide deterministic ${identity.groupId} Relic ${ordinal}.`],
      localizations: localizedNameDescription(name),
    };
  }
  throw new Error(`unsupported staged E2E item type: ${identity.itemType}`);
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
      const promptText = body.messages
        ?.map((message) => String(message.content ?? ""))
        .join("\n") ?? "";
      const isSuiteBrief = promptText.includes("Coordinate one bounded multi-item composition");
      const isStagedNode = promptText.includes("Define exactly one item for a precompiled composition node");
      if (isSuiteBrief) {
        const scenario = stagedScenario(promptText);
        const blueprint = taggedJson(promptText, "composition-blueprint");
        await recordStaged("composition_suite_brief", scenario);
        await completeResponse(
          response,
          JSON.stringify(suiteBrief(blueprint)),
          scenario === "controls" ? 1_500 : 50,
        );
        return;
      }
      if (isStagedNode) {
        const scenario = stagedScenario(promptText);
        const identity = taggedJson(promptText, "node-identity");
        const failureKey = `${scenario}:${identity.executionNodeId}`;
        const failOnce = scenario === "recovery"
          && identity.itemType === "character"
          && !stagedFailures.has(failureKey);
        if (failOnce) stagedFailures.add(failureKey);
        await recordStaged("composition_node", scenario, {
          nodeId: identity.executionNodeId,
          outcome: failOnce ? "invalid" : "valid",
        });
        await completeResponse(
          response,
          failOnce ? "{" : JSON.stringify(stagedNodeCheckpoint(identity)),
          scenario === "controls" ? 1_500 : 50,
        );
        return;
      }
      const isPlan = body.messages?.some((message) =>
        message.role === "system"
          && String(message.content).includes("Plan exactly one independently testable item"));
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
