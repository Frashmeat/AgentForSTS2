import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({ appType: "custom", server: { middlewareMode: true } });

after(async () => vite.close());

const {
  bindSelectedResource,
  candidatesForRole,
  resourceBindingIssues,
  workbenchRoles,
} = await vite.ssrLoadModule("/src/pages/resourceWorkbenchModel.ts");

const digest = "a".repeat(64);
const catalog = {
  gamePackId: "sts2",
  gamePackSha256: "b".repeat(64),
  roles: [
    { id: "relic.master", mediaTypes: ["image/png"], width: 512, height: 512, requireAlpha: true, source: { kind: "master" } },
    { id: "relic.normal", mediaTypes: ["image/png"], width: 128, height: 128, requireAlpha: true, targetPath: "x", source: { kind: "derived", sourceRole: "relic.master" } },
  ],
};
const asset = {
  schemaVersion: 2,
  resourceId: "resource.fixture",
  logicalRole: "relic.normal",
  origin: { kind: "user_upload" },
  originalVersion: digest,
  selectedVersion: digest,
  versions: [{
    id: digest,
    blob: { relativePath: "versions/x/original.png", mediaType: "image/png", byteLength: 4, sha256: digest, width: 128, height: 128, hasAlpha: true },
    provenance: { kind: "original" },
  }],
};
const definition = {
  schemaVersion: 1,
  itemId: "fixture-relic",
  itemType: "relic",
  canonicalFields: {},
  behaviorIntent: [],
  localizations: {},
  resourceBindings: {},
};

test("required derived roles include their Pack-owned master", () => {
  assert.deepEqual(workbenchRoles(catalog, ["relic.normal"]).map((role) => role.id), [
    "relic.master",
    "relic.normal",
  ]);
});

test("candidate grouping is driven only by logical role", () => {
  assert.deepEqual(candidatesForRole([asset], "relic.normal"), [asset]);
  assert.deepEqual(candidatesForRole([asset], "relic.master"), []);
});

test("only an explicitly selected candidate becomes an ItemDefinition binding", () => {
  const bound = bindSelectedResource(definition, "relic.normal", asset, digest);
  assert.deepEqual(bound.resourceBindings["relic.normal"], {
    resourceId: "resource.fixture",
    selectedVersion: digest,
  });
  assert.deepEqual(resourceBindingIssues(bound, ["relic.normal"], catalog, [asset]), []);
});

test("missing and stale bindings remain visible", () => {
  assert.deepEqual(resourceBindingIssues(definition, ["relic.normal"], catalog, [asset]), [
    "relic.normal: select a resource candidate.",
  ]);
  const stale = {
    ...definition,
    resourceBindings: { "relic.normal": { resourceId: asset.resourceId, selectedVersion: "c".repeat(64) } },
  };
  assert.deepEqual(resourceBindingIssues(stale, ["relic.normal"], catalog, [asset]), [
    "relic.normal: saved binding is stale or does not match the Pack shape.",
  ]);
});
