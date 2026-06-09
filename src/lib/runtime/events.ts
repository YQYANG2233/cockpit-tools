import {
  emit as tauriEmit,
  listen as tauriListen,
  TauriEvent,
  type Event,
  type UnlistenFn,
} from '@tauri-apps/api/event';
import { isTauriRuntime, loadRuntimeCapabilities } from './invoke';

export { TauriEvent };
export type { Event, UnlistenFn };

type EventHandler<T> = (event: Event<T>) => void | Promise<void>;

function parseEventPayload<T>(raw: string): T {
  try {
    return JSON.parse(raw) as T;
  } catch {
    return raw as T;
  }
}

export async function listen<T>(
  event: string,
  handler: EventHandler<T>,
): Promise<UnlistenFn> {
  if (isTauriRuntime()) {
    return tauriListen(event, handler);
  }

  const runtime = await loadRuntimeCapabilities();
  if (runtime.mode !== 'web-gateway' || !runtime.eventBaseUrl) {
    return () => {};
  }

  const source = new EventSource(runtime.eventBaseUrl);
  const listener = (message: MessageEvent<string>) => {
    void handler({
      event,
      id: 0,
      payload: parseEventPayload<T>(message.data),
    });
  };
  source.addEventListener(event, listener as EventListener);

  return () => {
    source.removeEventListener(event, listener as EventListener);
    source.close();
  };
}

export async function emit<T>(event: string, payload?: T): Promise<void> {
  if (isTauriRuntime()) {
    await tauriEmit(event, payload);
    return;
  }

  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(event, { detail: payload }));
  }
}
