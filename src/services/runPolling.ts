import { getRun, type RunRecord } from "./tauriApi";

export function isTerminal(run: Pick<RunRecord, "status">): boolean {
  return run.status === "succeeded" || run.status === "failed" || run.status === "cancelled";
}

export async function waitForRun(
  runId: string,
  onUpdate?: (run: RunRecord) => void,
  timeoutMs = 180_000,
): Promise<RunRecord> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const run = await getRun(runId);
    onUpdate?.(run);
    if (isTerminal(run)) return run;
    await new Promise((resolve) => window.setTimeout(resolve, 400));
  }
  throw new Error("run polling timed out");
}
