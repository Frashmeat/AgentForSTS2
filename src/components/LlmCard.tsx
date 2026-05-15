import { useRef, useState } from "react";
import { Badge, Button, Card, Field, Notice } from "@/components/ui";
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

  const [completeResp, setCompleteResp] = useState<CompletionResponse | null>(null);
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
    <Card
      eyebrow="llm · anthropic completion"
      title="LLM"
      actions={
        <>
          <Button size="sm" onClick={handleComplete} disabled={running}>
            {running && !streaming ? "Running…" : "Complete"}
          </Button>
          <Button
            size="sm"
            variant="accent"
            onClick={streaming ? handleCancel : handleStream}
            disabled={running && !streaming}
          >
            {streaming ? "Cancel" : "Stream"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label="system prompt (optional)">
          <input
            value={systemPrompt}
            onChange={(e) => setSystemPrompt(e.target.value)}
          />
        </Field>
        <Field label="user prompt">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={3}
          />
        </Field>
        <Field label="max tokens" className="max-w-[200px]">
          <input
            type="number"
            min={1}
            max={8192}
            value={maxTokens}
            onChange={(e) => setMaxTokens(Number(e.target.value) || 512)}
          />
        </Field>
      </div>

      {error && <Notice variant="error" title={`Error: ${error}`} className="mt-3" />}

      {completeResp && !showStream && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center gap-2 flex-wrap">
            <Badge variant="muted">model · {completeResp.model}</Badge>
            <Badge variant="muted">finish · {completeResp.finishReason}</Badge>
            <Badge variant="muted">
              tok in {completeResp.usage.inputTokens} / out{" "}
              {completeResp.usage.outputTokens}
            </Badge>
          </div>
          <pre className="pre-block max-h-96">{completeResp.content}</pre>
        </div>
      )}

      {showStream && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center gap-2 flex-wrap">
            <Badge variant="muted">model · {streamState.model ?? "—"}</Badge>
            {streamState.finishReason && (
              <Badge variant="muted">finish · {streamState.finishReason}</Badge>
            )}
            {streamState.usage && (
              <Badge variant="muted">
                tok in {streamState.usage.inputTokens} / out{" "}
                {streamState.usage.outputTokens}
              </Badge>
            )}
            {streaming && <Badge variant="running">streaming</Badge>}
          </div>
          <pre className="pre-block pre-block-stream max-h-96">
            {streamState.text || (streaming ? " " : "")}
          </pre>
        </div>
      )}
    </Card>
  );
}
