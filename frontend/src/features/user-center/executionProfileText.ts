interface ExecutionProfileLike {
  selected_agent_backend?: string;
  selected_model?: string;
}

function formatAgentBackendLabel(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (normalized === "codex") {
    return "Codex CLI";
  }
  if (normalized === "claude") {
    return "Claude CLI";
  }
  return value.trim();
}

export function formatExecutionProfileText(job: ExecutionProfileLike): string | null {
  const backend = formatAgentBackendLabel(String(job.selected_agent_backend ?? ""));
  const model = String(job.selected_model ?? "").trim();
  if (!backend && !model) {
    return null;
  }
  if (!backend) {
    return model;
  }
  if (!model) {
    return backend;
  }
  return `${backend} / ${model}`;
}
