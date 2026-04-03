export type SocketEvent = {
  event: string;
  stage: string;
} & Record<string, unknown>;

export function normalizeEvent(payload: Record<string, unknown>): SocketEvent {
  const event = String(payload.event ?? payload.stage ?? "unknown");
  return {
    ...payload,
    event,
    stage: String(payload.stage ?? event),
  } as SocketEvent;
}
