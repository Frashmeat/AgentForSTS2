// Runs 提交表单：按 kind 切换字段。原 RunsCard 里的提交部分独立成组件。
//
// 字段 state 留在本组件内部；提交成功通过 onSubmitted callback 通知父级（父级
// 一般触发 RunsList refresh）。错误冒泡给父级。

import { useState } from "react";
import { Button, Field, FieldRow } from "@/components/ui";
import { api } from "@/services/api";
import {
  localValidationFailure,
  toActionableFailure,
} from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type { SubmitRunAck } from "@/services/tauriApi";

export type SubmitKind =
  | "text_generate"
  | "code_generate_asset"
  | "code_generate_custom"
  | "asset_generate"
  | "batch_custom_code"
  | "build_project"
  | "package_project"
  | "log_analysis"
  | "truth_snapshot_refresh";

const KIND_LABELS: Array<{ value: SubmitKind; label: string }> = [
  { value: "text_generate", label: "text_generate (free prompt)" },
  { value: "code_generate_asset", label: "code_generate (asset)" },
  { value: "code_generate_custom", label: "code_generate (custom code)" },
  { value: "asset_generate", label: "asset_generate (image+code)" },
  { value: "batch_custom_code", label: "batch_custom_code" },
  { value: "build_project", label: "build_project (dotnet publish)" },
  { value: "package_project", label: "package_project (zip)" },
  { value: "log_analysis", label: "log_analysis (LLM diagnose)" },
  { value: "truth_snapshot_refresh", label: "truth_snapshot_refresh" },
];

interface Props {
  onSubmitted: (runId: string) => void;
  onError: (failure: ActionableFailure) => void;
}

export function RunsSubmitForm({ onSubmitted, onError }: Props) {
  const [submitKind, setSubmitKind] = useState<SubmitKind>("text_generate");
  const [busy, setBusy] = useState(false);

  const [prompt, setPrompt] = useState("用一句中文打招呼");
  const [assetType, setAssetType] = useState("card");
  const [assetName, setAssetName] = useState("DemoCard");
  const [designDescription, setDesignDescription] = useState("造成 10 点伤害，弃 1 张牌。");
  const [assetProjectRoot, setAssetProjectRoot] = useState("");
  const [customName, setCustomName] = useState("MyHook");
  const [customDescription, setCustomDescription] = useState("把 player.maxHp 翻倍");
  const [customImplNotes, setCustomImplNotes] = useState(
    "OverrideMember Player.GetMaxHp 返回原值 *2",
  );
  const [imagePrompt, setImagePrompt] = useState(
    "a fierce-looking card art, dark background, fantasy style",
  );
  const [buildProjectRoot, setBuildProjectRoot] = useState("");
  const [packageSourceDir, setPackageSourceDir] = useState("");
  const [packageOutputPath, setPackageOutputPath] = useState("");
  const [logText, setLogText] = useState("");
  const [logContextHint, setLogContextHint] = useState("");
  const [batchItemsJson, setBatchItemsJson] = useState(
    `[
  {
    "name": "HookOne",
    "description": "把 player 起手获得 1 点护甲",
    "implementation_notes": "OnBattleStart hook 加 BlockPower 1",
    "project_root": "",
    "skip_build": true
  }
]`,
  );
  const [batchFailFast, setBatchFailFast] = useState(false);
  const [snapshotForce, setSnapshotForce] = useState(false);

  async function handleSubmit() {
    // Client-side validation
    const invalid = (message: string) =>
      onError(localValidationFailure("runs.submit", message));
    if (submitKind === "text_generate" && !prompt.trim()) { invalid("Prompt 不能为空。"); return; }
    if ((submitKind === "code_generate_asset" || submitKind === "code_generate_custom") && (!customName.trim() || !customDescription.trim())) { invalid("Name 和 description 不能为空。"); return; }
    if (submitKind === "asset_generate" && (!imagePrompt.trim() || !assetName.trim() || !designDescription.trim())) { invalid("Image prompt、asset name 和 design description 不能为空。"); return; }
    if (submitKind === "batch_custom_code") {
      let items: unknown[];
      try { items = JSON.parse(batchItemsJson); } catch { invalid("Batch items JSON 格式错误。"); return; }
      if (!Array.isArray(items) || items.length === 0) { invalid("Batch items 数组不能为空。"); return; }
    }
    if (submitKind === "build_project" && !buildProjectRoot.trim()) { invalid("Project root 不能为空。"); return; }
    if (submitKind === "package_project" && !packageSourceDir.trim()) { invalid("Source directory 不能为空。"); return; }
    if (submitKind === "log_analysis" && !logText.trim()) { invalid("Log text 不能为空。"); return; }
    setBusy(true);
    try {
      let ack: SubmitRunAck;
      switch (submitKind) {
        case "text_generate":
          ack = (await api.submitTextGenerateRun({ prompt })) as SubmitRunAck;
          break;
        case "code_generate_asset":
          ack = (await api.submitCodeGenerateRun({
            mode: "asset",
            request: {
              asset_type: assetType,
              asset_name: assetName,
              design_description: designDescription,
              project_root: assetProjectRoot || ".",
              image_paths: [],
              name_zhs: "",
              skip_build: true,
            },
          })) as SubmitRunAck;
          break;
        case "code_generate_custom":
          ack = (await api.submitCodeGenerateRun({
            mode: "custom_code",
            request: {
              name: customName,
              description: customDescription,
              implementation_notes: customImplNotes,
              project_root: assetProjectRoot || ".",
              skip_build: true,
            },
          })) as SubmitRunAck;
          break;
        case "asset_generate":
          ack = (await api.submitAssetGenerateRun({
            asset_request: {
              asset_type: assetType,
              asset_name: assetName,
              design_description: designDescription,
              project_root: assetProjectRoot || ".",
              image_paths: [],
              name_zhs: "",
              skip_build: true,
            },
            image_prompt: imagePrompt.trim() || null,
          })) as SubmitRunAck;
          break;
        case "batch_custom_code": {
          let items;
          try {
            items = JSON.parse(batchItemsJson);
          } catch {
            throw localValidationFailure("runs.batch_items", "Batch items JSON 格式错误。");
          }
          if (!Array.isArray(items)) {
            throw localValidationFailure("runs.batch_items", "Batch items 必须是 JSON 数组。");
          }
          ack = (await api.submitBatchCustomCodeRun({
            items,
            fail_fast: batchFailFast,
          })) as SubmitRunAck;
          break;
        }
        case "build_project":
          ack = (await api.submitBuildProjectRun({
            project_root: buildProjectRoot,
            max_attempts: 3,
          })) as SubmitRunAck;
          break;
        case "package_project":
          ack = (await api.submitPackageProjectRun({
            source_dir: packageSourceDir,
            output_path: packageOutputPath.trim() || null,
          })) as SubmitRunAck;
          break;
        case "log_analysis":
          ack = (await api.submitLogAnalysisRun({
            log_text: logText.trim() || null,
            context_hint: logContextHint.trim() || null,
          })) as SubmitRunAck;
          break;
        case "truth_snapshot_refresh":
          ack = (await api.submitTruthSnapshotRefreshRun({
            force: snapshotForce,
          })) as SubmitRunAck;
          break;
      }
      onSubmitted(ack.runId);
    } catch (e: unknown) {
      onError(toActionableFailure(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-3">
      <Field label="run kind">
        <select
          value={submitKind}
          onChange={(e) => setSubmitKind(e.target.value as SubmitKind)}
          data-testid="run-kind"
        >
          {KIND_LABELS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </Field>

      {submitKind === "text_generate" && (
        <Field label="prompt">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={2}
          />
        </Field>
      )}

      {(submitKind === "code_generate_asset" ||
        submitKind === "asset_generate") && (
        <>
          <FieldRow>
            <Field label="asset type">
              <select
                value={assetType}
                onChange={(e) => setAssetType(e.target.value)}
              >
                {["card", "card_fullscreen", "relic", "power", "character"].map(
                  (t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ),
                )}
              </select>
            </Field>
            <Field label="asset name">
              <input
                value={assetName}
                onChange={(e) => setAssetName(e.target.value)}
              />
            </Field>
          </FieldRow>
          <Field label="project root (for prompt context)">
            <input
              value={assetProjectRoot}
              onChange={(e) => setAssetProjectRoot(e.target.value)}
              placeholder="E:/mods/demo_mod"
              className="input-mono"
            />
          </Field>
          <Field label="design description">
            <textarea
              value={designDescription}
              onChange={(e) => setDesignDescription(e.target.value)}
              rows={2}
            />
          </Field>
          {submitKind === "asset_generate" && (
            <Field
              label="image prompt"
              hint="留空跳过出图，仅跑 code"
            >
              <textarea
                value={imagePrompt}
                onChange={(e) => setImagePrompt(e.target.value)}
                rows={2}
              />
            </Field>
          )}
          <p style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
            Output → formal project files + immutable artifact manifest
            {submitKind === "asset_generate" && imagePrompt.trim() && (
              <>
                {" "}+ <code>{assetName}.png</code>
              </>
            )}
          </p>
        </>
      )}

      {submitKind === "code_generate_custom" && (
        <>
          <Field label="name (sanitized as class name)">
            <input
              value={customName}
              onChange={(e) => setCustomName(e.target.value)}
            />
          </Field>
          <Field label="project root">
            <input
              value={assetProjectRoot}
              onChange={(e) => setAssetProjectRoot(e.target.value)}
              placeholder="E:/mods/demo_mod"
              className="input-mono"
            />
          </Field>
          <Field label="description">
            <textarea
              value={customDescription}
              onChange={(e) => setCustomDescription(e.target.value)}
              rows={2}
            />
          </Field>
          <Field label="implementation notes">
            <textarea
              value={customImplNotes}
              onChange={(e) => setCustomImplNotes(e.target.value)}
              rows={2}
            />
          </Field>
        </>
      )}

      {submitKind === "batch_custom_code" && (
        <>
          <Field label="items (JSON array of CustomCodegenRequest)">
            <textarea
              value={batchItemsJson}
              onChange={(e) => setBatchItemsJson(e.target.value)}
              rows={8}
              className="input-mono"
            />
          </Field>
          <label className="flex items-center gap-2" style={{ fontSize: "13px" }}>
            <input
              type="checkbox"
              checked={batchFailFast}
              onChange={(e) => setBatchFailFast(e.target.checked)}
            />
            <span>fail_fast（首失立停）</span>
          </label>
        </>
      )}

      {submitKind === "build_project" && (
        <Field label="project root (will run `dotnet publish` here)">
          <input
            value={buildProjectRoot}
            onChange={(e) => setBuildProjectRoot(e.target.value)}
            placeholder="E:/mods/demo_mod/DemoMod"
            className="input-mono"
            data-testid="run-build-project-root"
          />
        </Field>
      )}

      {submitKind === "package_project" && (
        <>
          <Field label="source dir (要打包的目录)">
            <input
              value={packageSourceDir}
              onChange={(e) => setPackageSourceDir(e.target.value)}
              placeholder="E:/mods/demo_mod/artifacts"
              className="input-mono"
              data-testid="run-package-source-dir"
            />
          </Field>
          <Field
            label="output zip path"
            hint="留空 → 与 source_dir 同级 <name>-<ts>.zip"
          >
            <input
              value={packageOutputPath}
              onChange={(e) => setPackageOutputPath(e.target.value)}
              placeholder="E:/mods/demo_mod-release.zip"
              className="input-mono"
              data-testid="run-package-output-path"
            />
          </Field>
        </>
      )}

      {submitKind === "log_analysis" && (
        <>
          <Field label="build log（粘贴日志文本）">
            <textarea
              value={logText}
              onChange={(e) => setLogText(e.target.value)}
              rows={6}
              placeholder="把 dotnet publish 失败日志粘贴到这里…"
              className="input-mono"
            />
          </Field>
          <Field label="context hint" hint="可选，提示用户改了什么">
            <input
              value={logContextHint}
              onChange={(e) => setLogContextHint(e.target.value)}
              placeholder="我刚改了 TargetFramework 到 net9.0"
            />
          </Field>
        </>
      )}

      {submitKind === "truth_snapshot_refresh" && (
        <label className="flex items-center gap-2" style={{ fontSize: "13px" }}>
          <input
            type="checkbox"
            checked={snapshotForce}
            onChange={(event) => setSnapshotForce(event.target.checked)}
            data-testid="run-truth-snapshot-force"
          />
          <span>force re-index</span>
        </label>
      )}

      <Button
        variant="primary"
        onClick={() => void handleSubmit()}
        disabled={busy}
        data-testid="run-submit"
      >
        {busy ? "Submitting…" : "Submit"}
      </Button>
    </div>
  );
}
