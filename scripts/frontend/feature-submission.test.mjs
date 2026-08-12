import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  server: { middlewareMode: true },
});

after(async () => {
  await vite.close();
});

const { buildFeatureSubmission } = await vite.ssrLoadModule(
  "/src/services/featureSubmission.ts",
);

test("feature submissions preserve explicit breaking schema versions", () => {
  assert.deepEqual(
    buildFeatureSubmission(
      "mod.generate.single",
      "feature.mod-generate-single-request",
      { artifactId: "fixture" },
      3,
    ),
    {
      featureId: "mod.generate.single",
      request: {
        schema: { id: "feature.mod-generate-single-request", version: 3 },
        payload: { artifactId: "fixture" },
      },
    },
  );
  assert.deepEqual(
    buildFeatureSubmission(
      "composition.generate",
      "feature.composition-generate-request",
      { artifactId: "fixture-composition" },
      4,
    ).request.schema,
    { id: "feature.composition-generate-request", version: 4 },
  );
});

test("source paths remain outside the typed request payload", () => {
  const submission = buildFeatureSubmission(
    "resource.prepare",
    "feature.resource-prepare-request",
    { logicalRole: "relic.normal" },
    1,
    "input/icon.png",
  );
  assert.equal(submission.sourcePath, "input/icon.png");
  assert.equal("sourcePath" in submission.request.payload, false);
});
