import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("single asset platform payload uses item_name and excludes local-only fields", () => {
  const source = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");

  assert.match(source, /const singleAssetItem: PlatformJobCreateItem = \{/);
  assert.match(source, /item_name: assetName\.trim\(\)/);
  assert.doesNotMatch(source, /asset_name: assetName\.trim\(\)/);
  assert.doesNotMatch(source, /project_root: projectRoot\.trim\(\)/);
  assert.doesNotMatch(source, /has_uploaded_image:/);
});

test("batch platform payload uses item_name and excludes local-only fields", () => {
  const source = readSource("../src/features/batch-generation/view.tsx");
  const batchPayloadBuilderMatch = source.match(/input_payload:\s*\{[\s\S]*?depends_on: item\.depends_on,[\s\S]*?\},\s*\}\)\)/);

  assert.ok(batchPayloadBuilderMatch);
  const batchPayloadBuilder = batchPayloadBuilderMatch[0];
  assert.match(source, /item_name: item\.name/);
  assert.match(source, /asset_type: item\.type/);
  assert.doesNotMatch(batchPayloadBuilder, /(?:^|\s)name: item\.name/);
  assert.doesNotMatch(batchPayloadBuilder, /has_uploaded_image:/);
});

test("execution mode flow can block server mode for unsupported payloads", () => {
  const dialogSource = readSource("../src/components/ExecutionModeDialog.tsx");
  const flowSource = readSource("../src/features/workspace/useExecutionModeFlow.ts");
  const singleSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");
  const batchSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(dialogSource, /serverUnsupportedReasons/);
  assert.match(flowSource, /serverUnsupportedReasons/);
  assert.match(singleSource, /serverUploads:/);
  assert.match(batchSource, /serverUploads:/);
});
