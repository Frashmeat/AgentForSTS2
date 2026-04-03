import type { SocketEvent } from "./events.ts";
import { WorkflowClient } from "./client.ts";

export class WorkflowSocketFacade<TEvent extends SocketEvent> {
  protected client: WorkflowClient<TEvent>;

  constructor(path: string) {
    this.client = new WorkflowClient<TEvent>(path);
  }

  on<T extends TEvent["event"]>(
    event: T,
    handler: (data: Extract<TEvent, { event: T }>) => void
  ) {
    this.client.on(event, handler);
    return this;
  }

  send(data: object) {
    this.client.send(data);
  }

  waitOpen(): Promise<void> {
    return this.client.waitOpen();
  }

  attachPersistentErrorHandlers(onError: (message: string) => void) {
    this.client.attachPersistentErrorHandlers(onError);
    return this;
  }

  close() {
    this.client.close();
  }
}
