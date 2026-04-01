import test from "node:test";
import assert from "node:assert/strict";

import type { ApprovalRequest } from "../src/lib/approvals.ts";
import {
  SINGLE_ASSET_SNAPSHOT_KEY,
  loadSingleAssetSnapshot,
  refreshRecoveredSingleAssetApprovals,
  restoreSingleAssetSnapshot,
  saveSingleAssetSnapshot,
  serializeSingleAssetSnapshot,
} from "../src/features/single-asset/recovery.ts";
import {
  createInitialSingleAssetWorkflowState,
  type SingleAssetWorkflowState,
} from "../src/features/single-asset/state.ts";

function createApproval(overrides: Partial<ApprovalRequest> = {}): ApprovalRequest {
  return {
    action_id: "req-1",
    kind: "shell_command",
    title: "run build",
    reason: "verify",
    risk_level: "medium",
    requires_approval: true,
    status: "pending",
    payload: {},
    ...overrides,
  };
}

function createStorageStub() {
  const store = new Map<string, string>();
  return {
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, value);
    },
    removeItem(key: string) {
      store.delete(key);
    },
  };
}

test("serialize and restore single asset snapshot preserves draft and workflow state", () => {
  const workflowState: SingleAssetWorkflowState = {
    ...createInitialSingleAssetWorkflowState(),
    stage: "pick_image",
    images: ["img-b64"],
    promptPreview: "draw relic",
    negativePrompt: "blurry",
    currentPrompt: "draw relic final",
    showMorePrompt: true,
    genLog: ["g1"],
    flowStageCurrent: "正在生图",
    flowStageHistory: ["准备", "正在生图"],
  };

  const snapshot = serializeSingleAssetSnapshot({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: "E:/STS2mod",
    imageMode: "upload",
    autoMode: true,
    uploadedImageB64: "upload-b64",
    uploadedImageName: "test.png",
    uploadedImagePreview: "data:image/png;base64,upload-b64",
    workflowState,
  });

  const restored = restoreSingleAssetSnapshot(snapshot);

  assert.equal(snapshot.assetName, "DarkRelic");
  assert.equal(restored?.assetType, "relic");
  assert.equal(restored?.imageMode, "upload");
  assert.equal(restored?.uploadedImagePreview, "data:image/png;base64,upload-b64");
  assert.equal(restored?.workflowState.stage, "pick_image");
  assert.deepEqual(restored?.workflowState.images, ["img-b64"]);
});

test("saveSingleAssetSnapshot and loadSingleAssetSnapshot round-trip storage", () => {
  const storage = createStorageStub();

  saveSingleAssetSnapshot(storage, {
    assetType: "card",
    assetName: "DarkBlade",
    description: "desc",
    projectRoot: "E:/STS2mod",
    imageMode: "ai",
    autoMode: false,
    uploadedImageB64: "",
    uploadedImageName: "",
    uploadedImagePreview: null,
    workflowState: {
      ...createInitialSingleAssetWorkflowState(),
      stage: "confirm_prompt",
      promptPreview: "prompt",
    },
  });

  const restored = loadSingleAssetSnapshot(storage);

  assert.equal(storage.getItem(SINGLE_ASSET_SNAPSHOT_KEY) !== null, true);
  assert.equal(restored?.workflowState.stage, "confirm_prompt");
  assert.equal(restored?.promptPreview ?? restored?.workflowState.promptPreview, "prompt");
});

test("refreshRecoveredSingleAssetApprovals replaces stale approval requests from backend", async () => {
  const refreshed = await refreshRecoveredSingleAssetApprovals({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: "E:/STS2mod",
    imageMode: "ai",
    autoMode: false,
    uploadedImageB64: "",
    uploadedImageName: "",
    uploadedImagePreview: null,
    workflowState: {
      ...createInitialSingleAssetWorkflowState(),
      stage: "approval_pending",
      approvalSummary: "等待审批",
      approvalRequests: [createApproval()],
    },
  }, async (actionId) =>
    createApproval({ action_id: actionId, status: "approved", source_backend: "codex" }),
  );

  assert.equal(refreshed.workflowState.stage, "approval_pending");
  assert.equal(refreshed.workflowState.approvalRequests[0].status, "approved");
  assert.equal(refreshed.workflowState.approvalRequests[0].source_backend, "codex");
});

test("restoreSingleAssetSnapshot rejects invalid payloads safely", () => {
  assert.equal(restoreSingleAssetSnapshot({ version: 999 }), null);
  assert.equal(restoreSingleAssetSnapshot(null), null);
});
