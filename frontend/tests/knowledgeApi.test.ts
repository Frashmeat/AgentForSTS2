import test from "node:test";
import assert from "node:assert/strict";

import { exportCurrentKnowledgePack } from "../src/shared/api/index.ts";

function setWorkstationApiBase() {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
  });
}

test("exportCurrentKnowledgePack downloads workstation knowledge zip", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return {
        ok: true,
        headers: new Headers({
          "Content-Disposition": 'attachment; filename="current-knowledge.zip"',
          "X-ATS-Knowledge-Pack-File-Count": "3",
        }),
        async blob() {
          return new Blob(["zip-bytes"], { type: "application/zip" });
        },
        async text() {
          return "";
        },
      };
    },
  });

  const result = await exportCurrentKnowledgePack();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/knowledge/export-pack");
  assert.equal(calls[0].init?.credentials, "include");
  assert.equal(result.fileName, "current-knowledge.zip");
  assert.equal(result.fileCount, 3);
  assert.equal(await result.blob.text(), "zip-bytes");
});
