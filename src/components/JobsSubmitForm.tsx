// Jobs 提交表单：按 kind 切换字段。原 JobsCard 里的提交部分独立成组件。
//
// 字段 state 留在本组件内部；提交成功通过 onSubmitted callback 通知父级（父级
// 一般触发 JobsList refresh）。错误冒泡给父级。

import { useState } from "react";
import { api } from "@/services/api";
import type { SubmitJobAck } from "@/services/tauriApi";

export type SubmitKind =
  | "text_generate"
  | "code_generate_asset"
  | "code_generate_custom"
  | "asset_generate"
  | "batch_custom_code"
  | "build_project"
  | "package_project"
  | "log_analysis"
  | "knowledge_refresh";

const KIND_LABELS: Array<{ value: SubmitKind; label: string }> = [
  { value: "text_generate", label: "text_generate (free prompt)" },
  { value: "code_generate_asset", label: "code_generate (asset)" },
  { value: "code_generate_custom", label: "code_generate (custom code)" },
  { value: "asset_generate", label: "asset_generate (image+code)" },
  { value: "batch_custom_code", label: "batch_custom_code" },
  { value: "build_project", label: "build_project (dotnet publish)" },
  { value: "package_project", label: "package_project (zip)" },
  { value: "log_analysis", label: "log_analysis (LLM diagnose)" },
  { value: "knowledge_refresh", label: "knowledge_refresh (ilspycmd)" },
];

interface Props {
  onSubmitted: (jobId: string) => void;
  onError: (message: string) => void;
}

export function JobsSubmitForm({ onSubmitted, onError }: Props) {
  const [submitKind, setSubmitKind] = useState<SubmitKind>("text_generate");
  const [busy, setBusy] = useState(false);

  // 各 kind 字段
  const [prompt, setPrompt] = useState("用一句中文打招呼");
  const [assetType, setAssetType] = useState("card");
  const [assetName, setAssetName] = useState("DemoCard");
  const [designDescription, setDesignDescription] = useState(
    "造成 10 点伤害，弃 1 张牌。",
  );
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
  const [knowledgeDllPath, setKnowledgeDllPath] = useState("");
  const [knowledgeForce, setKnowledgeForce] = useState(false);
  const [knowledgeIncludeBaselib, setKnowledgeIncludeBaselib] = useState(false);

  async function handleSubmit() {
    setBusy(true);
    try {
      let ack: SubmitJobAck;
      switch (submitKind) {
        case "text_generate":
          ack = (await api.submitTextGenerateJob({ prompt })) as SubmitJobAck;
          break;
        case "code_generate_asset":
          ack = (await api.submitCodeGenerateJob({
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
          })) as SubmitJobAck;
          break;
        case "code_generate_custom":
          ack = (await api.submitCodeGenerateJob({
            mode: "custom_code",
            request: {
              name: customName,
              description: customDescription,
              implementation_notes: customImplNotes,
              project_root: assetProjectRoot || ".",
              skip_build: true,
            },
          })) as SubmitJobAck;
          break;
        case "asset_generate":
          ack = (await api.submitAssetGenerateJob({
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
          })) as SubmitJobAck;
          break;
        case "batch_custom_code": {
          let items;
          try {
            items = JSON.parse(batchItemsJson);
          } catch (e) {
            throw new Error(`batch items JSON parse failed: ${String(e)}`);
          }
          if (!Array.isArray(items)) {
            throw new Error("batch items must be a JSON array");
          }
          ack = (await api.submitBatchCustomCodeJob({
            items,
            fail_fast: batchFailFast,
          })) as SubmitJobAck;
          break;
        }
        case "build_project":
          ack = (await api.submitBuildProjectJob({
            project_root: buildProjectRoot,
            max_attempts: 3,
          })) as SubmitJobAck;
          break;
        case "package_project":
          ack = (await api.submitPackageProjectJob({
            source_dir: packageSourceDir,
            output_path: packageOutputPath.trim() || null,
          })) as SubmitJobAck;
          break;
        case "log_analysis":
          ack = (await api.submitLogAnalysisJob({
            log_text: logText.trim() || null,
            context_hint: logContextHint.trim() || null,
          })) as SubmitJobAck;
          break;
        case "knowledge_refresh":
          ack = (await api.submitKnowledgeRefreshJob({
            sts2_dll_path: knowledgeDllPath,
            force: knowledgeForce,
            include_baselib: knowledgeIncludeBaselib,
          })) as SubmitJobAck;
          break;
      }
      onSubmitted(ack.jobId);
    } catch (e: unknown) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2">
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-muted text-xs">Job kind</span>
        <select
          value={submitKind}
          onChange={(e) => setSubmitKind(e.target.value as SubmitKind)}
          className="px-2 py-1 rounded border border-muted/30 bg-transparent"
        >
          {KIND_LABELS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </label>

      {submitKind === "text_generate" && (
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Prompt</span>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={2}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent"
          />
        </label>
      )}

      {(submitKind === "code_generate_asset" ||
        submitKind === "asset_generate") && (
        <>
          <div className="grid grid-cols-2 gap-2 text-sm">
            <label className="flex flex-col gap-1">
              <span className="text-muted text-xs">Asset type</span>
              <select
                value={assetType}
                onChange={(e) => setAssetType(e.target.value)}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              >
                {["card", "card_fullscreen", "relic", "power", "character"].map(
                  (t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ),
                )}
              </select>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-muted text-xs">Asset name</span>
              <input
                value={assetName}
                onChange={(e) => setAssetName(e.target.value)}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
          </div>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">
              Project root (for prompt context)
            </span>
            <input
              value={assetProjectRoot}
              onChange={(e) => setAssetProjectRoot(e.target.value)}
              placeholder="E:/mods/demo_mod"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Design description</span>
            <textarea
              value={designDescription}
              onChange={(e) => setDesignDescription(e.target.value)}
              rows={2}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
          {submitKind === "asset_generate" && (
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">
                Image prompt（留空跳过出图，仅跑 code）
              </span>
              <textarea
                value={imagePrompt}
                onChange={(e) => setImagePrompt(e.target.value)}
                rows={2}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
          )}
          <p className="text-xs text-muted">
            Output → <code>artifacts/{assetName}/{assetName}.cs</code>
            {submitKind === "asset_generate" && imagePrompt.trim() && (
              <>
                {" "}
                + <code>{assetName}.png</code>
              </>
            )}
          </p>
        </>
      )}

      {submitKind === "code_generate_custom" && (
        <>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">
              Name (sanitized as class name)
            </span>
            <input
              value={customName}
              onChange={(e) => setCustomName(e.target.value)}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Project root</span>
            <input
              value={assetProjectRoot}
              onChange={(e) => setAssetProjectRoot(e.target.value)}
              placeholder="E:/mods/demo_mod"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Description</span>
            <textarea
              value={customDescription}
              onChange={(e) => setCustomDescription(e.target.value)}
              rows={2}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Implementation notes</span>
            <textarea
              value={customImplNotes}
              onChange={(e) => setCustomImplNotes(e.target.value)}
              rows={2}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
        </>
      )}

      {submitKind === "batch_custom_code" && (
        <>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">
              Items (JSON array of CustomCodegenRequest)
            </span>
            <textarea
              value={batchItemsJson}
              onChange={(e) => setBatchItemsJson(e.target.value)}
              rows={8}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex items-center gap-2 text-sm">
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
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">
            Project root (will run `dotnet publish` here)
          </span>
          <input
            value={buildProjectRoot}
            onChange={(e) => setBuildProjectRoot(e.target.value)}
            placeholder="E:/mods/demo_mod/DemoMod"
            className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
          />
        </label>
      )}

      {submitKind === "package_project" && (
        <>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Source dir (要打包的目录)</span>
            <input
              value={packageSourceDir}
              onChange={(e) => setPackageSourceDir(e.target.value)}
              placeholder="E:/mods/demo_mod/artifacts"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">
              Output zip path (留空 → 与 source_dir 同级 &lt;name&gt;-&lt;ts&gt;.zip)
            </span>
            <input
              value={packageOutputPath}
              onChange={(e) => setPackageOutputPath(e.target.value)}
              placeholder="E:/mods/demo_mod-release.zip"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
        </>
      )}

      {submitKind === "log_analysis" && (
        <>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Build log（粘贴日志文本）</span>
            <textarea
              value={logText}
              onChange={(e) => setLogText(e.target.value)}
              rows={6}
              placeholder="把 dotnet publish 失败日志粘贴到这里…"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">
              Context hint（可选，提示用户改了什么）
            </span>
            <input
              value={logContextHint}
              onChange={(e) => setLogContextHint(e.target.value)}
              placeholder="我刚改了 TargetFramework 到 net9.0"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
        </>
      )}

      {submitKind === "knowledge_refresh" && (
        <>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">sts2.dll path</span>
            <input
              value={knowledgeDllPath}
              onChange={(e) => setKnowledgeDllPath(e.target.value)}
              placeholder="E:/Steam/steamapps/common/SlayTheSpire2/sts2.dll"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
          <div className="flex flex-wrap gap-4 text-sm">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={knowledgeForce}
                onChange={(e) => setKnowledgeForce(e.target.checked)}
              />
              <span>force（忽略 manifest 缓存重新反编译）</span>
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={knowledgeIncludeBaselib}
                onChange={(e) => setKnowledgeIncludeBaselib(e.target.checked)}
              />
              <span>include_baselib（同时拉 BaseLib.dll 反编译）</span>
            </label>
          </div>
        </>
      )}

      <button
        type="button"
        onClick={() => void handleSubmit()}
        disabled={busy}
        className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
      >
        {busy ? "Submitting…" : "Submit"}
      </button>
    </div>
  );
}
