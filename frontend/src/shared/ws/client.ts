import { normalizeEvent, type SocketEvent } from "./events.ts";

type EventName<TEvent extends SocketEvent> = TEvent["event"];
type EventHandler<TEvent extends SocketEvent> = (data: TEvent) => void;

export class WorkflowClient<TEvent extends SocketEvent = SocketEvent> {
  private ws: WebSocket;
  private listeners = new Map<string, EventHandler<TEvent>[]>();
  private intentionallyClosed = false;

  constructor(path: string) {
    this.ws = new WebSocket(`ws://${location.host}${path}`);
    this.ws.onmessage = (event) => {
      const normalized = normalizeEvent(JSON.parse(event.data)) as TEvent;
      const handlers = this.listeners.get(normalized.event) ?? [];
      handlers.forEach((handler) => handler(normalized));
    };
  }

  on(event: EventName<TEvent>, handler: EventHandler<TEvent>) {
    const list = this.listeners.get(event) ?? [];
    this.listeners.set(event, [...list, handler]);
    return this;
  }

  send(data: object) {
    this.ws.send(JSON.stringify(data));
  }

  waitOpen(): Promise<void> {
    if (this.ws.readyState === WebSocket.OPEN) {
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      const cleanup = () => {
        this.ws.onopen = null;
        this.ws.onerror = null;
        this.ws.onclose = null;
      };
      const timer = setTimeout(() => {
        cleanup();
        reject(new Error("WebSocket connection timed out"));
      }, 5000);

      this.ws.onopen = () => {
        clearTimeout(timer);
        cleanup();
        resolve();
      };
      this.ws.onerror = () => {
        clearTimeout(timer);
        cleanup();
        reject(new Error("WebSocket connection failed"));
      };
      this.ws.onclose = () => {
        clearTimeout(timer);
        cleanup();
        reject(new Error("WebSocket connection closed"));
      };
    });
  }

  attachPersistentErrorHandlers(onError: (message: string) => void) {
    this.ws.onerror = () => {
      if (this.intentionallyClosed) return;
      onError("WebSocket 连接出错，与后端的连接已中断");
    };
    this.ws.onclose = (event: CloseEvent) => {
      if (this.intentionallyClosed) return;
      if (!event.wasClean) {
        onError(`WebSocket 连接意外断开（code: ${event.code}），后端可能已崩溃或进程退出`);
      }
    };
  }

  close() {
    this.intentionallyClosed = true;
    this.ws.close();
  }
}
