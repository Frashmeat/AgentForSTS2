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

function imageDimensions(size) {
  const match = /^([1-9]\d{0,4})x([1-9]\d{0,4})$/u.exec(String(size));
  if (!match) throw new Error(`invalid Images API size: ${size}`);
  const width = Number(match[1]);
  const height = Number(match[2]);
  if (width > 4_096 || height > 4_096 || width * height > 16_777_216) {
    throw new Error(`Images API size exceeds the E2E fixture limit: ${size}`);
  }
  return { width, height };
}

function deterministicSubjectPng(width, height) {
  const scanlines = Buffer.alloc(height * (1 + width * 4));
  for (let y = 0; y < height; y += 1) {
    const row = y * (1 + width * 4);
    scanlines[row] = 0;
    for (let x = 0; x < width; x += 1) {
      const offset = row + 1 + x * 4;
      const subject = x >= width / 4 && x < width * 3 / 4
        && y >= height / 4 && y < height * 3 / 4;
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
const compositionSingleFailures = new Set();

function taggedJson(promptText, tag) {
  const match = promptText.match(new RegExp(`<${tag}>\\s*([\\s\\S]*?)\\s*</${tag}>`));
  if (!match) throw new Error(`${tag} was not present in the staged prompt`);
  return JSON.parse(match[1]);
}

function taggedText(promptText, tag) {
  const match = promptText.match(new RegExp(`<${tag}>\\s*([\\s\\S]*?)\\s*</${tag}>`));
  if (!match || !match[1].trim()) throw new Error(`${tag} was not present in the staged prompt`);
  return match[1].trim();
}

function stagedScenario(promptText) {
  if (promptText.includes("GUI staged control-state E2E")) return "controls";
  if (promptText.includes("GUI staged recovery E2E")) return "recovery";
  return "default";
}

function isModGenerateSingleRequest(body, promptText) {
  const schema = body.response_format?.json_schema?.schema;
  return promptText.includes("<item-definition>")
    && promptText.includes("<pack-contribution>")
    && schema?.type === "object"
    && schema?.properties?.files?.type === "object"
    && schema?.properties?.acceptanceNotes?.type === "array"
    && schema?.additionalProperties === false;
}

if (!isModGenerateSingleRequest({
  response_format: {
    json_schema: {
      schema: {
        type: "object",
        additionalProperties: false,
        properties: {
          files: { type: "object" },
          acceptanceNotes: { type: "array" },
        },
      },
    },
  },
}, "<pack-contribution>{}</pack-contribution><item-definition>{}</item-definition>")) {
  throw new Error("mod.generate.single E2E request classifier self-test failed");
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
        visual_profile: { kind: "choice", value: "placeholder" },
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

function modelType(itemId) {
  const value = String(itemId)
    .split(/[^A-Za-z0-9]+/u)
    .filter(Boolean)
    .map((part) => `${part[0].toUpperCase()}${part.slice(1)}`)
    .join("");
  return `E2e${/^\d/u.test(value) ? `Item${value}` : value}`;
}

function compositionGeneratePlan(promptText) {
  const itemType = taggedText(promptText, "requested-item-type");
  if (!["character", "card", "relic"].includes(itemType)) {
    throw new Error(`unsupported composition Generate plan item type: ${itemType}`);
  }
  return {
    ...plan,
    itemId: `e2e-${itemType}`,
    itemType,
    name: `E2E ${itemType}`,
    summary: `Deterministic ${itemType} plan for recoverable composition GUI E2E.`,
  };
}

function bindings(definition, slotId, kind) {
  return (definition.referenceBindings?.[slotId] ?? []).filter((binding) => binding.kind === kind);
}

function localization(prefix, values) {
  return Object.fromEntries(
    Object.entries(values).map(([suffix, value]) => [`E2EMOD-${prefix}.${suffix}`, value]),
  );
}

function localizationPrefix(itemId) {
  return modelType(itemId)
    .replace(/([a-z0-9])([A-Z])/gu, "$1_$2")
    .toUpperCase();
}

function architectLocalization(prefix, title) {
  return {
    [`THE_ARCHITECT.talk.E2EMOD-${prefix}.0-0r.char`]: title,
    [`THE_ARCHITECT.talk.E2EMOD-${prefix}.0-0r.next`]: "Continue",
    [`THE_ARCHITECT.talk.E2EMOD-${prefix}.0-1r.ancient`]: "The Architect answers.",
    [`THE_ARCHITECT.talk.E2EMOD-${prefix}.0-attack`]: "Both",
  };
}

function compositionCharacterSource(definition) {
  const type = modelType(definition.itemId);
  const startingDeck = bindings(definition, "starting_deck", "pinned")
    .flatMap((binding) => Array.from(
      { length: binding.quantity },
      () => `ModelDb.Card<${modelType(binding.itemId)}>()`,
    ));
  const startingRelics = bindings(definition, "starting_relics", "pinned")
    .map((binding) => `ModelDb.Relic<${modelType(binding.itemId)}>()`);
  const fields = definition.canonicalFields;
  const gender = String(fields.gender.value);
  const genderName = `${gender[0].toUpperCase()}${gender.slice(1)}`;
  return `using BaseLib.Abstracts;
using Godot;
using MegaCrit.Sts2.Core.Entities.Characters;
using MegaCrit.Sts2.Core.Models;

namespace E2EMod;

public sealed class ${type}CardPool : CustomCardPoolModel
{
    public override string Title => "e2e";
    public override bool IsColorless => false;
    public override Color ShaderColor => new("${fields.name_color.value}");
    public override Color DeckEntryCardColor => new("${fields.name_color.value}");
}

public sealed class ${type}RelicPool : CustomRelicPoolModel { }
public sealed class ${type}PotionPool : CustomPotionPoolModel { }

public sealed class ${type} : PlaceholderCharacterModel
{
    public override string PlaceholderID => "${fields.placeholder_id.value}";
    public override Color NameColor => new("${fields.name_color.value}");
    public override CharacterGender Gender => CharacterGender.${genderName};
    public override int StartingHp => ${fields.starting_hp.value};
    public override int StartingGold => ${fields.starting_gold.value};
    public override int MaxEnergy => ${fields.max_energy.value};
    public override CardPoolModel CardPool => ModelDb.CardPool<${type}CardPool>();
    public override RelicPoolModel RelicPool => ModelDb.RelicPool<${type}RelicPool>();
    public override PotionPoolModel PotionPool => ModelDb.PotionPool<${type}PotionPool>();
    public override IEnumerable<CardModel> StartingDeck => [${startingDeck.join(", ")}];
    public override IReadOnlyList<RelicModel> StartingRelics => [${startingRelics.join(", ")}];
}`;
}

function compositionCardSource(definition) {
  const type = modelType(definition.itemId);
  const owner = bindings(definition, "owner_character", "identity")[0];
  if (!owner) throw new Error(`card ${definition.itemId} has no owner_character`);
  const ownerType = modelType(owner.itemId);
  const fields = definition.canonicalFields;
  const cardType = fields.card_type.value === "skill" ? "Skill" : "Attack";
  const target = fields.target.value === "self" ? "Self" : "AnyEnemy";
  const rarity = `${String(fields.rarity.value)[0].toUpperCase()}${String(fields.rarity.value).slice(1)}`;
  return `using BaseLib.Abstracts;
using BaseLib.Utils;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;

namespace E2EMod;

[Pool(typeof(${ownerType}CardPool))]
public sealed class ${type}() : CustomCardModel(${fields.base_cost.value}, CardType.${cardType}, CardRarity.${rarity}, TargetType.${target})
{
    protected override Task OnPlay(PlayerChoiceContext choiceContext, CardPlay cardPlay) => Task.CompletedTask;
}`;
}

function compositionRelicSource(definition) {
  const type = modelType(definition.itemId);
  const owner = bindings(definition, "owner_character", "identity")[0];
  if (!owner) throw new Error(`relic ${definition.itemId} has no owner_character`);
  const ownerType = modelType(owner.itemId);
  const rarity = `${String(definition.canonicalFields.rarity.value)[0].toUpperCase()}${String(definition.canonicalFields.rarity.value).slice(1)}`;
  return `using BaseLib.Abstracts;
using BaseLib.Utils;
using MegaCrit.Sts2.Core.Entities.Relics;

namespace E2EMod;

[Pool(typeof(${ownerType}RelicPool))]
public sealed class ${type} : CustomRelicModel
{
    public override RelicRarity Rarity => RelicRarity.${rarity};
}`;
}

function compositionBundle(stored) {
  const definition = stored.definition;
  const prefix = localizationPrefix(definition.itemId);
  if (definition.itemType === "character") {
    const characterValues = (locale) => {
      const fields = definition.localizations[locale].fields;
      return {
        title: fields.title,
        titleObject: fields.title_object,
        description: fields.description,
        pronounObject: fields.pronoun_object,
        pronounSubject: fields.pronoun_subject,
        pronounPossessive: fields.pronoun_possessive,
        possessiveAdjective: fields.possessive_adjective,
        aromaPrinciple: fields.aroma_principle,
        "banter.alive.endTurnPing": fields.end_turn_ping_alive,
        "banter.dead.endTurnPing": fields.end_turn_ping_dead,
        eventDeathPrevention: fields.event_death_prevention,
        goldMonologue: fields.gold_monologue,
        cardsModifierTitle: fields.cards_modifier_title,
        cardsModifierDescription: fields.cards_modifier_description,
      };
    };
    return {
      files: {
        source: compositionCharacterSource(definition),
        "localization.eng": localization(prefix, characterValues("eng")),
        "localization.zhs": localization(prefix, characterValues("zhs")),
        "localization.ancients.eng": architectLocalization(
          prefix,
          definition.localizations.eng.fields.title,
        ),
        "localization.ancients.zhs": architectLocalization(
          prefix,
          definition.localizations.zhs.fields.title,
        ),
      },
      acceptanceNotes: ["Generated for recoverable composition GUI E2E."],
    };
  }
  const localizedValues = (locale) => definition.localizations[locale].fields;
  if (definition.itemType === "card") {
    return {
      files: {
        source: compositionCardSource(definition),
        "localization.eng": localization(prefix, {
          title: localizedValues("eng").name,
          description: localizedValues("eng").description,
        }),
        "localization.zhs": localization(prefix, {
          title: localizedValues("zhs").name,
          description: localizedValues("zhs").description,
        }),
      },
      acceptanceNotes: ["Generated for recoverable composition GUI E2E."],
    };
  }
  if (definition.itemType === "relic") {
    return {
      files: {
        source: compositionRelicSource(definition),
        "localization.eng": localization(prefix, {
          title: localizedValues("eng").name,
          description: localizedValues("eng").description,
          flavor: "Deterministic E2E relic.",
        }),
        "localization.zhs": localization(prefix, {
          title: localizedValues("zhs").name,
          description: localizedValues("zhs").description,
          flavor: "Deterministic E2E relic.",
        }),
      },
      acceptanceNotes: ["Generated for recoverable composition GUI E2E."],
    };
  }
  throw new Error(`unsupported composition Generate item type: ${definition.itemType}`);
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
      const isSingle = isModGenerateSingleRequest(body, promptText);
      if (isSingle
        && promptText.includes("<item-definition>")
        && /"itemType"\s*:\s*"(?:character|card|relic)"/u.test(promptText)) {
        const stored = taggedJson(promptText, "item-definition");
        const itemId = stored.definition.itemId;
        const itemType = stored.definition.itemType;
        const failOnce = itemType === "character" && !compositionSingleFailures.has(itemId);
        if (failOnce) compositionSingleFailures.add(itemId);
        await recordStaged("composition_generate_single", "generate", {
          itemId,
          itemType,
          outcome: failOnce ? "invalid" : "valid",
        });
        await completeResponse(
          response,
          failOnce ? "{" : JSON.stringify(compositionBundle(stored)),
          50,
        );
        return;
      }
      const isPlan = body.messages?.some((message) =>
        message.role === "system"
          && String(message.content).includes("Plan exactly one independently testable item"));
      const isCompileFailure = promptText.includes("CompileFailureRelic");
      const isCompositionGeneratePlan = isPlan
        && (promptText.includes("Provide deterministic")
          || promptText.includes("Provide a playable deterministic"));
      const planResponse = isCompositionGeneratePlan
        ? compositionGeneratePlan(promptText)
        : plan;
      if (isCompositionGeneratePlan) {
        await recordStaged("composition_generate_plan", "generate", {
          itemType: planResponse.itemType,
        });
      } else {
        await record(isPlan ? "plan" : isCompileFailure ? "asset_bundle_compile_failure" : "asset_bundle");
      }
      await completeResponse(
        response,
        JSON.stringify(isPlan ? planResponse : isCompileFailure ? compileFailureBundle : bundle),
        isPlan ? 1_200 : 100,
      );
      return;
    }
    if (request.method === "POST" && request.url === "/v1/images/generations") {
      const body = await readJson(request);
      const { width, height } = imageDimensions(body.size);
      const pngBase64 = deterministicSubjectPng(width, height).toString("base64");
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
