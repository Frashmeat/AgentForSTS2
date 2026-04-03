import { resolveErrorMessage } from "../../shared/error.ts";

export interface BatchPlanningSocketSessionLike {
  waitOpen(): Promise<void>;
  send(data: object): void;
  close(): void;
}

interface OpenBatchPlanningSocketOptions {
  requirements?: string;
  projectRoot?: string;
  payload?: object;
  onOpenError(message: string): void;
}

export async function openBatchPlanningSocket(
  socket: BatchPlanningSocketSessionLike,
  options: OpenBatchPlanningSocketOptions,
): Promise<boolean> {
  try {
    await socket.waitOpen();
  } catch (error) {
    socket.close();
    options.onOpenError(resolveErrorMessage(error));
    return false;
  }

  const payload = options.payload ?? {
    action: "start",
    requirements: options.requirements ?? "",
    project_root: options.projectRoot ?? "",
  };
  socket.send(payload);
  return true;
}
