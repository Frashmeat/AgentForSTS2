interface ExecutionProfileLike {
  selected_runner_type?: string;
  selected_model?: string;
}

function formatRunnerTypeLabel(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (normalized === "codex_cli") {
    return "Codex CLI";
  }
  if (normalized === "claude_cli") {
    return "Claude CLI";
  }
  if (normalized === "api") {
    return "API";
  }
  return value.trim();
}

export function formatExecutionProfileText(job: ExecutionProfileLike): string | null {
  const backend = formatRunnerTypeLabel(String(job.selected_runner_type ?? ""));
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
