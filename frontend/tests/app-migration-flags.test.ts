import test from "node:test";
import assert from "node:assert/strict";

import { resolveMigrationFlags } from "../src/shared/api/index.ts";

test("resolveMigrationFlags defaults to legacy-safe switches", () => {
  assert.deepEqual(resolveMigrationFlags(undefined), {
    use_modular_single_workflow: false,
    use_modular_batch_workflow: false,
    use_unified_ws_contract: false,
  });
});

test("resolveMigrationFlags keeps per-flag overrides independent", () => {
  assert.deepEqual(
    resolveMigrationFlags({
      migration: {
        use_modular_single_workflow: true,
        use_unified_ws_contract: true,
      },
    }),
    {
      use_modular_single_workflow: true,
      use_modular_batch_workflow: false,
      use_unified_ws_contract: true,
    }
  );
});
