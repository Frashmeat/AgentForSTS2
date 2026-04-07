export type WorkflowLogChannel = "raw" | "stderr" | "system" | "stage";

export interface WorkflowLogEntry {
  text: string;
  source?: string;
  channel?: WorkflowLogChannel;
  model?: string;
}

export function appendWorkflowLogEntry(
  entries: WorkflowLogEntry[],
  entry: WorkflowLogEntry,
): WorkflowLogEntry[] {
  const previous = entries[entries.length - 1];
  if (
    previous &&
    previous.source === entry.source &&
    previous.channel === entry.channel &&
    previous.model === entry.model
  ) {
    return [
      ...entries.slice(0, -1),
      {
        ...previous,
        text: `${previous.text}${entry.text}`,
      },
    ];
  }
  return [...entries, entry];
}

export function resolveNextWorkflowModel(
  currentModel: string | null,
  entry: WorkflowLogEntry,
): string | null {
  return entry.model?.trim() ? entry.model : currentModel;
}

export function buildRawWorkflowLogLines(entries: WorkflowLogEntry[]): string[] {
  return entries.map((entry) => entry.text);
}

export function buildPrettyWorkflowLogLines(entries: WorkflowLogEntry[]): string[] {
  const lines: string[] = [];
  for (const entry of entries) {
    const rawText = entry.text ?? "";
    if (!rawText.trim()) {
      continue;
    }
    const text = entry.channel === "stderr" && !rawText.startsWith("[stderr]")
      ? `[stderr] ${rawText}`
      : rawText;
    if (lines[lines.length - 1] === text) {
      continue;
    }
    lines.push(text);
  }
  return lines;
}
