import type { WorkflowMigrationFlags } from "../shared/api/index.ts";
import { WorkflowSocket, type WsEvent } from "./ws.ts";

export type SingleAssetSocket = {
  on<T extends WsEvent["event"]>(
    event: T,
    handler: (data: Extract<WsEvent, { event: T }>) => void
  ): SingleAssetSocket;
  send(data: object): void;
  waitOpen(): Promise<void>;
  close(): void;
};

export class UnifiedWorkflowSocket extends WorkflowSocket {}

export function createSingleAssetSocket(flags: WorkflowMigrationFlags): SingleAssetSocket {
  if (flags.use_unified_ws_contract) {
    return new UnifiedWorkflowSocket();
  }
  return new WorkflowSocket();
}
