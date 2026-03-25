import type { WorkflowEvent } from "../types/workflow.ts";
import { WorkflowClient } from "./client.ts";

export class WorkflowSocketFacade<TEvent extends WorkflowEvent> {
  protected client: WorkflowClient;

  constructor(path: string) {
    this.client = new WorkflowClient(path);
  }

  on<T extends TEvent["event"]>(
    event: T,
    handler: (data: Extract<TEvent, { event: T }>) => void
  ) {
    this.client.on(event, handler as (data: WorkflowEvent) => void);
    return this;
  }

  send(data: object) {
    this.client.send(data);
  }

  waitOpen(): Promise<void> {
    return this.client.waitOpen();
  }

  close() {
    this.client.close();
  }
}
