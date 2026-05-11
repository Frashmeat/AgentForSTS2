import { useRef, useState } from "react";
import { api } from "@/services/api";
import { streamLlmCompletion, type StreamHandle } from "@/services/llmStream";
import type {
  CompletionRequest,
  CompletionResponse,
  FinishReason,
  Usage,
} from "@/services/tauriApi";

const DEFAULT_PROMPT = "用一句中文解释 Slay the Spire 2 的卡组构筑乐趣。";

interface StreamState {
  text: string;
  model: string | null;
  finishReason: FinishReason | null;
  usage: Usage | null;
}

const INITIAL_STREAM_STATE: StreamState = {
  text: "",
  model: null,
  finishReason: null,
  usage: null,
};

export function LlmCard() {
  const [prompt, setPrompt] = useState(DEFAULT_PROMPT);
  const [systemPrompt, setSystemPrompt] = useState("");
  const [maxTokens, setMaxTokens] = useState(512);

  const [completeResp, setCompleteResp] = useState<CompletionResponse | null>(
    null,
  );
  const [streamState, setStreamState] = useState<StreamState>(INITIAL_STREAM_STATE);
  const [streaming, setStreaming] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const handleRef = useRef<StreamHandle | null>(null);

  function buildRequest(): CompletionRequest {
    return {
      messages: [{ role: "user", content: prompt }],
      system_prompt: systemPrompt || null,
      max_tokens: maxTokens,
      temperature: null,
      model: null,
    };
  }

  async function handleComplete() {
    setRunning(true);
    setError(null);
    setCompleteResp(null);
    setStreamState(INITIAL_STREAM_STATE);
    try {
      const result = (await api.llmComplete(buildRequest())) as CompletionResponse;
      setCompleteResp(result);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  function handleStream() {
    setRunning(true);
    setStreaming(true);
    setError(null);
    setCompleteResp(null);
    setStreamState(INITIAL_STREAM_STATE);

    handleRef.current = streamLlmCompletion(buildRequest(), {
      onStart: (model) => setStreamState((prev) => ({ ...prev, model })),
      onDelta: (delta) =>
        setStreamState((prev) => ({ ...prev, text: prev.text + delta })),
      onEnd: (finishReason, usage) => {
        setStreamState((prev) => ({ ...prev, finishReason, usage }));
        setStreaming(false);
        setRunning(false);
      },
      onError: (message) => {
        setError(message);
        setStreaming(false);
        setRunning(false);
      },
    });
  }

  function handleCancel() {
    handleRef.current?.cancel();
    handleRef.current = null;
    setStreaming(false);
    setRunning(false);
  }

  const showStream = streamState.text.length > 0 || streamState.model !== null;

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">LLM — Anthropic Completion</h2>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={handleComplete}
            disabled={running}
            className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
          >
            {running && !streaming ? "Running…" : "Complete"}
          </button>
          <button
            type="button"
            onClick={streaming ? handleCancel : handleStream}
            disabled={running && !streaming}
            className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
          >
            {streaming ? "Cancel" : "Stream"}
          </button>
        </div>
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

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      {completeResp && !showStream && (
        <div className="mt-2 space-y-2">
          <p className="text-xs text-muted">
            Model: <span className="font-mono">{completeResp.model}</span>
            <span className="ml-3">Finish:</span>{" "}
            <span className="font-mono">{completeResp.finishReason}</span>
            <span className="ml-3">Tokens:</span>{" "}
            <span className="font-mono">
              in {completeResp.usage.inputTokens} / out{" "}
              {completeResp.usage.outputTokens}
            </span>
          </p>
          <pre className="text-sm p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {completeResp.content}
          </pre>
        </div>
      )}

      {showStream && (
        <div className="mt-2 space-y-2">
          <p className="text-xs text-muted">
            Model: <span className="font-mono">{streamState.model ?? "—"}</span>
            {streamState.finishReason && (
              <>
                <span className="ml-3">Finish:</span>{" "}
                <span className="font-mono">{streamState.finishReason}</span>
              </>
            )}
            {streamState.usage && (
              <>
                <span className="ml-3">Tokens:</span>{" "}
                <span className="font-mono">
                  in {streamState.usage.inputTokens} / out{" "}
                  {streamState.usage.outputTokens}
                </span>
              </>
            )}
            {streaming && <span className="ml-3 text-accent">streaming…</span>}
          </p>
          <pre className="text-sm p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {streamState.text || (streaming ? " " : "")}
          </pre>
        </div>
      )}
    </section>
  );
}
