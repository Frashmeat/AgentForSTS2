import { useState } from "react";
import { api } from "@/services/api";
import type { CompletionRequest, CompletionResponse } from "@/services/tauriApi";

const DEFAULT_PROMPT = "用一句中文解释 Slay the Spire 2 的卡组构筑乐趣。";

export function LlmCard() {
  const [prompt, setPrompt] = useState(DEFAULT_PROMPT);
  const [systemPrompt, setSystemPrompt] = useState("");
  const [maxTokens, setMaxTokens] = useState(512);
  const [response, setResponse] = useState<CompletionResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  async function handleComplete() {
    setRunning(true);
    setError(null);
    try {
      const request: CompletionRequest = {
        messages: [{ role: "user", content: prompt }],
        system_prompt: systemPrompt || null,
        max_tokens: maxTokens,
        temperature: null,
        model: null,
      };
      const result = (await api.llmComplete(request)) as CompletionResponse;
      setResponse(result);
    } catch (e: unknown) {
      setError(String(e));
      setResponse(null);
    } finally {
      setRunning(false);
    }
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">LLM — Anthropic Completion</h2>
        <button
          type="button"
          onClick={handleComplete}
          disabled={running}
          className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {running ? "Generating…" : "Complete"}
        </button>
      </div>

      <label className="flex flex-col gap-1 text-sm mb-2">
        <span className="text-muted text-xs">System prompt (optional)</span>
        <input
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          className="px-2 py-1 rounded border border-muted/30 bg-transparent"
        />
      </label>

      <label className="flex flex-col gap-1 text-sm mb-2">
        <span className="text-muted text-xs">User prompt</span>
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={3}
          className="px-2 py-1 rounded border border-muted/30 bg-transparent"
        />
      </label>

      <label className="flex flex-col gap-1 text-sm mb-3 max-w-[160px]">
        <span className="text-muted text-xs">Max tokens</span>
        <input
          type="number"
          min={1}
          max={8192}
          value={maxTokens}
          onChange={(e) => setMaxTokens(Number(e.target.value) || 512)}
          className="px-2 py-1 rounded border border-muted/30 bg-transparent"
        />
      </label>

      {error && (
        <p className="text-red-500 text-sm">Error: {error}</p>
      )}

      {response && (
        <div className="mt-2 space-y-2">
          <p className="text-xs text-muted">
            Model: <span className="font-mono">{response.model}</span>
            <span className="ml-3">Finish:</span>{" "}
            <span className="font-mono">{response.finishReason}</span>
            <span className="ml-3">Tokens:</span>{" "}
            <span className="font-mono">
              in {response.usage.inputTokens} / out {response.usage.outputTokens}
            </span>
          </p>
          <pre className="text-sm p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {response.content}
          </pre>
        </div>
      )}
    </section>
  );
}
