import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({ appType: "custom", server: { middlewareMode: true } });
after(async () => vite.close());

const {
  buildBatchRequest,
  buildComplexRequest,
  decodeGenerationComposition,
  failedRequestItems,
  unprocessedRequestItems,
} = await vite.ssrLoadModule("/src/pages/batchGenerationModel.ts");
const { isStoredItemDefinition } = await vite.ssrLoadModule("/src/services/itemContractGuards.ts");

const hash = "a".repeat(64);
const manifestHash = "b".repeat(64);
const definition = {
  definitionHash: hash,
  definition: {
    schemaVersion: 2,
    itemId: "fixture-item",
    itemType: "relic",
    canonicalFields: {},
    behaviorIntent: ["Create a fixture relic."],
    localizations: {},
    resourceBindings: {},
    referenceBindings: {},
  },
};

function run(featureId, requestSchema, request, resultSchema, result) {
  return {
    schemaVersion: 3,
    id: "parent-run",
    featureId,
    status: "succeeded",
    createdAt: "2026-08-04T00:00:00Z",
    startedAt: "2026-08-04T00:00:01Z",
    completedAt: "2026-08-04T00:00:02Z",
    request: { schema: requestSchema, payload: request },
    progress: null,
    failure: null,
    result: { schema: resultSchema, payload: result },
    attempts: 1,
    timeline: [],
  };
}

test("library definitions deterministically build Batch and Complex requests", () => {
  const batch = buildBatchRequest("FixtureMod", [definition], false);
  assert.equal(batch.items[0].artifactId, "fixture-item");
  assert.equal(batch.items[0].definition.definitionHash, hash);
  const complex = buildComplexRequest(batch, {
    artifactId: "fixture-package",
    modId: "FixtureMod",
    sourceRelativeRoot: "delivery",
    outputRelativePath: "packages/FixtureMod.zip",
  });
  assert.equal(complex.batch, batch);
});

test("ItemDefinition v2 guard rejects stale schema and malformed typed references", () => {
  assert.equal(isStoredItemDefinition(definition), true);
  const stale = structuredClone(definition);
  stale.definition.schemaVersion = 1;
  assert.equal(isStoredItemDefinition(stale), false);
  const malformed = structuredClone(definition);
  malformed.definition.referenceBindings.starting_deck = [{
    kind: "pinned",
    itemId: "fixture-card",
    definitionHash: "invalid",
    quantity: 1,
  }];
  assert.equal(isStoredItemDefinition(malformed), false);
});

test("Batch result decodes child Runs and pins failed retry input", () => {
  const request = buildBatchRequest("FixtureMod", [definition], false);
  const result = {
    total: 1,
    processed: 1,
    succeeded: 0,
    failed: 1,
    items: [{
      itemId: "fixture-item",
      definitionHash: hash,
      planRunId: "plan-run",
      generationRunId: "single-run",
      status: "failed",
      plan: {
        itemId: "fixture-item",
        itemType: "relic",
        name: "Fixture",
        summary: "Fixture",
        behaviorIntent: [],
        implementationConstraints: [],
        evidenceRequirements: [],
        requiredResourceRoles: [],
        acceptanceCriteria: [],
      },
      failureCode: "model.output_invalid",
    }],
  };
  const decoded = decodeGenerationComposition(run(
    "mod.generate.batch",
    { id: "feature.mod-generate-batch-request", version: 4 },
    request,
    { id: "feature.mod-generate-batch-result", version: 2 },
    result,
  ));
  assert.deepEqual(decoded.childRunIds, ["plan-run", "single-run"]);
  assert.equal(failedRequestItems(decoded)[0].definition, definition);
  assert.deepEqual(unprocessedRequestItems(decoded), []);
});

test("fail-fast exposes unprocessed definitions without inventing child Runs", () => {
  const second = structuredClone(definition);
  second.definitionHash = "c".repeat(64);
  second.definition.itemId = "second-item";
  const request = buildBatchRequest("FixtureMod", [definition, second], true);
  const result = {
    total: 2,
    processed: 1,
    succeeded: 1,
    failed: 0,
    items: [{
      itemId: "fixture-item",
      definitionHash: hash,
      planRunId: "plan-run",
      generationRunId: "single-run",
      status: "succeeded",
      plan: {
        itemId: "fixture-item", itemType: "relic", name: "Fixture", summary: "Fixture",
        behaviorIntent: [], implementationConstraints: [], evidenceRequirements: [],
        requiredResourceRoles: [], acceptanceCriteria: [],
      },
      result: {
        artifactManifestRef: "artifacts/fixture/manifest.json",
        manifestSha256: manifestHash,
        generatedFileCount: 1,
        validationPrimitive: "code.dotnet-validate",
        acceptanceNotes: [],
      },
    }],
  };
  const decoded = decodeGenerationComposition(run(
    "mod.generate.batch",
    { id: "feature.mod-generate-batch-request", version: 4 }, request,
    { id: "feature.mod-generate-batch-result", version: 2 }, result,
  ));
  assert.equal(unprocessedRequestItems(decoded)[0].definition.definition.itemId, "second-item");
});

test("malformed schema or counters are rejected", () => {
  const request = buildBatchRequest("FixtureMod", [definition], false);
  const malformed = { total: 1, processed: 1, succeeded: 1, failed: 1, items: [] };
  assert.equal(decodeGenerationComposition(run(
    "mod.generate.batch",
    { id: "feature.mod-generate-batch-request", version: 3 }, request,
    { id: "feature.mod-generate-batch-result", version: 2 }, malformed,
  )), null);
});
