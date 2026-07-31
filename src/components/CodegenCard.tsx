import { useState } from "react";
import { Button, Card, Field, FieldRow } from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { api } from "@/services/api";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
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
  const [error, setError] = useState<ActionableFailure | null>(null);
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
      setError(toActionableFailure(e));
      setPrompt(null);
    } finally {
      setRunning(false);
    }
  }

  return (
    <Card
      eyebrow="codegen · prompt preview"
      title="Codegen — Asset Prompt"
      actions={
        <Button size="sm" onClick={handleAssemble} disabled={running}>
          {running ? "Assembling…" : "Assemble"}
        </Button>
      }
    >
      <div className="space-y-3">
        <FieldRow>
          <Field label="asset type">
            <select
              value={assetType}
              onChange={(e) => setAssetType(e.target.value)}
            >
              {ASSET_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </Field>
          <Field label="asset name">
            <input
              value={assetName}
              onChange={(e) => setAssetName(e.target.value)}
            />
          </Field>
        </FieldRow>
        <FieldRow>
          <Field label="name (zhs)">
            <input
              value={nameZhs}
              onChange={(e) => setNameZhs(e.target.value)}
            />
          </Field>
          <Field label="project root">
            <input
              value={projectRoot}
              onChange={(e) => setProjectRoot(e.target.value)}
              className="input-mono"
            />
          </Field>
        </FieldRow>
        <Field label="design description">
          <textarea
            value={designDescription}
            onChange={(e) => setDesignDescription(e.target.value)}
            rows={3}
          />
        </Field>
        <label className="flex items-center gap-2" style={{ fontSize: "13px" }}>
          <input
            type="checkbox"
            checked={skipBuild}
            onChange={(e) => setSkipBuild(e.target.checked)}
          />
          <span>skip_build</span>
        </label>
      </div>

      <ActionableErrorNotice failure={error} className="mt-3" />

      {prompt && (
        <details open className="mt-4">
          <summary
            className="cursor-pointer mb-2"
            style={{
              fontFamily: '"JetBrains Mono", monospace',
              fontSize: "10.5px",
              letterSpacing: "0.16em",
              textTransform: "uppercase",
              color: "var(--ink-mute)",
            }}
          >
            Assembled prompt ({prompt.length} chars)
          </summary>
          <pre className="pre-block max-h-96">{prompt}</pre>
        </details>
      )}
    </Card>
  );
}
