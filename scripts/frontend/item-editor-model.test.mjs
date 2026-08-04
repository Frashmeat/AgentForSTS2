import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({ appType: "custom", server: { middlewareMode: true } });

after(async () => vite.close());

const {
  capabilityReason,
  confirmLocalization,
  createItemDraft,
  draftIssues,
  setBehaviorText,
  setFieldValue,
  setLocalizationText,
} = await vite.ssrLoadModule("/src/pages/itemEditorModel.ts");

const capability = {
  ready: true,
  blockers: [],
  descriptor: {
    id: "relic",
    displayNames: { eng: "Relic", zhs: "遗物" },
    requiredLocales: ["eng", "zhs"],
    requiredResourceRoles: [],
    evidenceQueries: [],
    fields: [{
      id: "rarity",
      displayNames: { eng: "Rarity" },
      required: true,
      value: { kind: "choice", options: [{ value: "common", displayNames: { eng: "Common" } }] },
    }],
  },
};

test("Pack descriptor creates the generic canonical draft without type branches", () => {
  const draft = createItemDraft(capability, "fixture-relic");
  assert.equal(draft.itemType, "relic");
  assert.deepEqual(draft.canonicalFields.rarity, { kind: "choice", value: "common" });
});

test("field and behavior edits return new typed drafts", () => {
  const original = createItemDraft(capability, "fixture-relic");
  const withField = setFieldValue(original, "rarity", { kind: "choice", value: "rare" });
  const withBehavior = setBehaviorText(withField, "First effect\n\n Second effect ");
  assert.notEqual(withField, original);
  assert.deepEqual(original.canonicalFields.rarity, { kind: "choice", value: "common" });
  assert.deepEqual(withField.canonicalFields.rarity, { kind: "choice", value: "rare" });
  assert.deepEqual(withBehavior.behaviorIntent, ["First effect", "Second effect"]);
});

test("editing the source locale marks confirmed translations outdated", () => {
  let draft = createItemDraft(capability, "fixture-relic");
  draft = setLocalizationText(draft, "eng", "eng", { name: "Fixture", description: "Source" });
  draft = setLocalizationText(draft, "zhs", "eng", { name: "测试", description: "译文" });
  draft = confirmLocalization(draft, "zhs");
  assert.equal(draft.localizations.zhs.status, "confirmed");
  draft = setLocalizationText(draft, "zhs", "eng", { description: "人工修订" });
  assert.equal(draft.localizations.zhs.status, "outdated");
  draft = confirmLocalization(draft, "zhs");
  draft = setLocalizationText(draft, "eng", "eng", { description: "Changed source" });
  assert.equal(draft.localizations.zhs.status, "outdated");
});

test("a translation cannot be saved before its declared source locale", () => {
  let draft = createItemDraft(capability, "fixture-relic");
  draft = setLocalizationText(draft, "zhs", "eng", {
    name: "测试",
    description: "译文",
  });
  assert.deepEqual(draftIssues(draft, capability.descriptor), [
    "zhs translation source eng must be created first.",
  ]);
});

test("blocked capability exposes a safe actionable reason", () => {
  assert.equal(capabilityReason({
    ...capability,
    ready: false,
    blockers: [{ code: "truth.evidence_missing", queryIndex: 2 }],
  }), "1 Truth evidence group missing");
});
