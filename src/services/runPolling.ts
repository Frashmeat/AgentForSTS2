import { getRun, type RunRecord } from "./tauriApi";

export interface RunPollingOptions {
  timeoutMs?: number;
  intervalMs?: number;
  readRun?: (runId: string) => Promise<RunRecord>;
  sleep?: (delayMs: number) => Promise<void>;
  now?: () => number;
}

export function isTerminal(run: Pick<RunRecord, "status">): boolean {
  return run.status === "succeeded" || run.status === "failed" || run.status === "cancelled";
}

export async function waitForRun(
  runId: string,
  onUpdate?: (run: RunRecord) => void,
  options: RunPollingOptions = {},
): Promise<RunRecord> {
  const readRun = options.readRun ?? getRun;
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((delayMs: number) => new Promise<void>(
    (resolve) => globalThis.setTimeout(resolve, delayMs),
  ));
  const intervalMs = options.intervalMs ?? 400;
  const deadline = options.timeoutMs === undefined ? null : now() + options.timeoutMs;

  while (deadline === null || now() < deadline) {
    const run = await readRun(runId);
    onUpdate?.(run);
    if (isTerminal(run)) return run;
    await sleep(intervalMs);
  }
  throw new Error("run polling timed out");
}
