import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({ appType: "custom", server: { middlewareMode: true } });
after(async () => vite.close());

const {
  buildCompositionPlanRequest,
  buildCompositionRetryNodeRequest,
  buildCompositionGenerateRequest,
  closedSelection,
  compositionFailureSummary,
  compositionRoots,
  defaultProfileChoice,
  draftRows,
  pageRows,
  parametersForChoice,
  profileIssues,
  replaceDraftNodeDefinition,
} = await vite.ssrLoadModule("/src/pages/compositionStudioModel.ts");
const { isCompositionConfirmation, isCompositionDraft } = await vite.ssrLoadModule(
  "/src/services/itemContractGuards.ts",
);

const profile = {
  id: "fixture_suite",
  displayNames: { eng: "Fixture suite" },
  rootItemType: "root",
  defaultProfile: "standard",
  customBaseProfile: "standard",
  maxNodes: 8,
  baseNodeCount: 1,
  parameters: [
    { id: "child_count", displayNames: { eng: "Children" }, min: 1, max: 4, nodeWeight: 1 },
  ],
  profiles: [
    { id: "prototype", displayNames: { eng: "Prototype" }, values: { child_count: 1 } },
    { id: "standard", displayNames: { eng: "Standard" }, values: { child_count: 3 } },
  ],
  constraints: [],
};

const hash = "a".repeat(64);
const definition = (itemId, itemType, referenceBindings = {}) => ({
  schemaVersion: 2,
  itemId,
  itemType,
  canonicalFields: {},
  behaviorIntent: ["Fixture intent"],
  localizations: {},
  resourceBindings: {},
  referenceBindings,
});

const draft = {
  schemaVersion: 2,
  draftId: "fixture-draft",
  revision: 1,
  gamePackId: "fixture-game",
  gamePackSha256: hash,
  rootItemId: "fixture-root",
  profile: {
    compositionId: "fixture_suite",
    source: { kind: "preset", profileId: "standard" },
    parameters: { child_count: 3 },
  },
  nodes: {
    "fixture-root": {
      definition: definition("fixture-root", "root", {
        children: [{ kind: "pinned", itemId: "fixture-child", definitionHash: hash, quantity: 1 }],
      }),
    },
    "fixture-child": {
      definition: definition("fixture-child", "child", {
        owner: [{ kind: "identity", itemId: "fixture-root", expectedItemType: "root" }],
      }),
      expectedCurrentDefinitionHash: hash,
    },
  },
  createdAt: "2026-08-05T00:00:00Z",
  updatedAt: "2026-08-05T00:00:00Z",
};

test("Pack default profile initializes Standard and Custom from its declared base", () => {
  const choice = defaultProfileChoice(profile);
  assert.deepEqual(choice, { kind: "preset", profileId: "standard" });
  assert.deepEqual(parametersForChoice(profile, choice), { child_count: 3 });
  assert.deepEqual(
    parametersForChoice(profile, { kind: "custom", baseProfileId: "standard" }, { child_count: 4 }),
    { child_count: 4 },
  );
});

test("profile bounds and run request remain Pack-driven", () => {
  assert.deepEqual(profileIssues(profile, { child_count: 5 }), [
    "Children must be between 1 and 4.",
  ]);
  assert.deepEqual(
    buildCompositionPlanRequest(
      " fixture-draft ",
      profile,
      { kind: "custom", baseProfileId: "standard" },
      { child_count: 4 },
      " Fixture concept ",
    ),
    {
      draftId: "fixture-draft",
      compositionId: "fixture_suite",
      concept: "Fixture concept",
      source: { kind: "custom", baseProfileId: "standard" },
      parameters: { child_count: 4 },
    },
  );
});

test("Draft filtering, pagination, and partial closure are deterministic", () => {
  assert.deepEqual(draftRows(draft, "child", "replacement", "fixture").map((row) => row.itemId), [
    "fixture-child",
  ]);
  assert.deepEqual(pageRows([1, 2, 3], 2, 2), { rows: [3], page: 2, pageCount: 2 });
  assert.equal(closedSelection(draft, new Set(["fixture-root"])), false);
  assert.equal(closedSelection(draft, new Set(["fixture-root", "fixture-child"])), true);
  assert.equal(closedSelection(draft, new Set(["fixture-child"])), true);
});

test("Draft resource edits replace only the selected definition and preserve CAS baselines", () => {
  const resourceDefinition = {
    ...draft.nodes["fixture-child"].definition,
    resourceBindings: {
      "child.icon": { resourceId: "child-icon", selectedVersion: hash },
    },
  };
  const nodes = replaceDraftNodeDefinition(draft, "fixture-child", resourceDefinition);
  assert.deepEqual(nodes["fixture-child"], {
    definition: resourceDefinition,
    expectedCurrentDefinitionHash: hash,
  });
  assert.equal(nodes["fixture-root"], draft.nodes["fixture-root"]);
  assert.equal(replaceDraftNodeDefinition(
    draft,
    "fixture-child",
    { ...resourceDefinition, itemId: "other-child" },
  ), draft.nodes);
});

test("targeted retry pins the Draft revision and trims only runtime instructions", () => {
  assert.deepEqual(buildCompositionRetryNodeRequest(
    draft,
    "fixture-child",
    "  revise the child behavior  ",
  ), {
    draftId: "fixture-draft",
    expectedRevision: 1,
    itemId: "fixture-child",
    instructions: "revise the child behavior",
  });
});

test("confirmed roots build one exact whole-closure request", () => {
  const root = {
    definitionHash: hash,
    definition: {
      ...definition("fixture-root", "root"),
      compositionProfile: draft.profile,
    },
  };
  const child = { definitionHash: hash, definition: definition("fixture-child", "child") };
  assert.deepEqual(compositionRoots([child, root], [profile]), [root]);
  assert.deepEqual(buildCompositionGenerateRequest(
    " fixture-composition ",
    " FixtureMod ",
    root,
    draft,
    {
      sourceRelativeRoot: " delivery ",
      outputRelativePath: " packages/FixtureMod.zip ",
      compressionLevel: 6,
    },
    { kind: "max_rounds", maxRounds: 3 },
  ), {
    artifactId: "fixture-composition",
    modId: "FixtureMod",
    root,
    draft: { draftId: "fixture-draft", revision: 1 },
    package: {
      artifactId: "fixture-composition",
      modId: "FixtureMod",
      sourceRelativeRoot: "delivery",
      outputRelativePath: "packages/FixtureMod.zip",
      compressionLevel: 6,
    },
    repairPolicy: { kind: "max_rounds", maxRounds: 3 },
  });
});

test("IPC guards reject malformed Draft and confirmation payloads", () => {
  assert.equal(isCompositionDraft(draft), true);
  assert.equal(isCompositionDraft({ ...draft, revision: 0 }), false);
  assert.equal(isCompositionDraft({ ...draft, nodes: { broken: { definition: { itemId: "x" } } } }), false);
  assert.equal(isCompositionConfirmation({
    draft: { draftId: "fixture-draft", revision: 1 },
    definitions: [{ definitionHash: hash, definition: definition("fixture-child", "child") }],
    confirmationDigest: hash,
  }), true);
  assert.equal(isCompositionConfirmation({ draft: {}, definitions: [], confirmationDigest: hash }), false);
});

test("composition failure details remain bounded and actionable", () => {
  assert.equal(compositionFailureSummary({
    code: "composition.profile.count_mismatch",
    stage: "composition.plan.model",
    details: {
      schema: { id: "feature.composition-plan-failure-details", version: 1 },
      payload: {
        reasonCode: "reference_total_quantity",
        itemId: "fixture-root",
        itemType: "root",
        slotId: "children",
        expectedCount: 10,
        actualCount: 9,
      },
    },
  }), "composition.plan.model · reference_total_quantity · item=fixture-root · type=root · slot=children · expected=10 · actual=9");
  assert.equal(compositionFailureSummary({
    code: "model.output_invalid",
    stage: "composition.plan.model",
    details: {
      schema: { id: "feature.composition-plan-failure-details", version: 1 },
      payload: { reasonCode: "unsafe value from provider" },
    },
  }), "composition.plan.model");
});
