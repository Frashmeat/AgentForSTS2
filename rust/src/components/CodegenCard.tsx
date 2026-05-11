import { useState } from "react";
import { api } from "@/services/api";
import type { AssetCodegenRequest } from "@/services/tauriApi";

const ASSET_TYPES = ["card", "card_fullscreen", "relic", "power", "character"];

export function CodegenCard() {
  const [assetType, setAssetType] = useState("card");
  const [assetName, setAssetName] = useState("DemoCard");
  const [nameZhs, setNameZhs] = useState("演示卡");
  const [projectRoot, setProjectRoot] = useState("E:/mods/demo_mod");
  const [designDescription, setDesignDescription] = useState(
    "造成 10 点伤害，弃 1 张牌。",
  );
  const [skipBuild, setSkipBuild] = useState(false);

  const [prompt, setPrompt] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  async function handleAssemble() {
    setRunning(true);
    setError(null);
    try {
      const request: AssetCodegenRequest = {
        asset_type: assetType,
        asset_name: assetName,
        name_zhs: nameZhs,
        project_root: projectRoot,
        design_description: designDescription,
        image_paths: [],
        skip_build: skipBuild,
      };
      const result = (await api.codegenAssetPrompt(request)) as string;
      setPrompt(result);
    } catch (e: unknown) {
      setError(String(e));
      setPrompt(null);
    } finally {
      setRunning(false);
    }
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">Codegen — Asset Prompt Preview</h2>
        <button
          type="button"
          onClick={handleAssemble}
          disabled={running}
          className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {running ? "Assembling…" : "Assemble"}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3 text-sm mb-3">
        <label className="flex flex-col gap-1">
          <span className="text-muted text-xs">Asset type</span>
          <select
            value={assetType}
            onChange={(e) => setAssetType(e.target.value)}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent"
          >
            {ASSET_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
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
        <label className="flex flex-col gap-1">
          <span className="text-muted text-xs">Name (zhs)</span>
          <input
            value={nameZhs}
            onChange={(e) => setNameZhs(e.target.value)}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-muted text-xs">Project root</span>
          <input
            value={projectRoot}
            onChange={(e) => setProjectRoot(e.target.value)}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
          />
        </label>
      </div>

      <label className="flex flex-col gap-1 text-sm mb-3">
        <span className="text-muted text-xs">Design description</span>
        <textarea
          value={designDescription}
          onChange={(e) => setDesignDescription(e.target.value)}
          rows={3}
          className="px-2 py-1 rounded border border-muted/30 bg-transparent"
        />
      </label>

      <label className="flex items-center gap-2 text-sm mb-3">
        <input
          type="checkbox"
          checked={skipBuild}
          onChange={(e) => setSkipBuild(e.target.checked)}
        />
        <span>skip_build</span>
      </label>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      {prompt && (
        <details open className="mt-3">
          <summary className="cursor-pointer text-sm text-muted mb-2">
            Assembled prompt ({prompt.length} chars)
          </summary>
          <pre className="text-xs p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {prompt}
          </pre>
        </details>
      )}
    </section>
  );
}
