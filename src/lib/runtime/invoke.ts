import {
  invoke as tauriInvoke,
  isTauri as tauriIsTauri,
} from '@tauri-apps/api/core';

type InvokeParams = Record<string, unknown> | null | undefined;

type RuntimeCapabilities = {
  mode: 'desktop' | 'web-gateway' | 'unsupported-web';
  rpcBaseUrl: string;
  eventBaseUrl?: string;
  serviceHost?: string;
  features?: Record<string, unknown>;
};

type JsonRpcResponse<T> = {
  jsonrpc: string;
  id: string;
  result?: T;
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
};

const DEFAULT_RUNTIME: RuntimeCapabilities = {
  mode: 'unsupported-web',
  rpcBaseUrl: '/api/rpc',
  eventBaseUrl: '/api/events',
};

const DESKTOP_RUNTIME: RuntimeCapabilities = {
  mode: 'desktop',
  rpcBaseUrl: '',
};

let runtimeCache: RuntimeCapabilities | null = null;
let runtimePromise: Promise<RuntimeCapabilities> | null = null;

export function isTauriRuntime(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  return tauriIsTauri();
}

function normalizeRuntime(value: unknown): RuntimeCapabilities {
  if (!value || typeof value !== 'object') {
    return DEFAULT_RUNTIME;
  }
  const raw = value as Record<string, unknown>;
  const mode =
    raw.mode === 'web-gateway' || raw.mode === 'desktop'
      ? raw.mode
      : 'unsupported-web';
  return {
    mode,
    rpcBaseUrl:
      typeof raw.rpcBaseUrl === 'string' && raw.rpcBaseUrl.trim()
        ? raw.rpcBaseUrl
        : '/api/rpc',
    eventBaseUrl:
      typeof raw.eventBaseUrl === 'string' && raw.eventBaseUrl.trim()
        ? raw.eventBaseUrl
        : '/api/events',
    serviceHost:
      typeof raw.serviceHost === 'string' && raw.serviceHost.trim()
        ? raw.serviceHost
        : undefined,
    features:
      raw.features && typeof raw.features === 'object'
        ? (raw.features as Record<string, unknown>)
        : undefined,
  };
}

export async function loadRuntimeCapabilities(force = false): Promise<RuntimeCapabilities> {
  if (isTauriRuntime()) {
    runtimeCache = DESKTOP_RUNTIME;
    return DESKTOP_RUNTIME;
  }
  if (!force && runtimeCache) {
    return runtimeCache;
  }
  if (!force && runtimePromise) {
    return runtimePromise;
  }
  runtimePromise = (async () => {
    try {
      const response = await fetch(`/api/runtime?_=${Date.now()}`, {
        cache: 'no-store',
        headers: { accept: 'application/json' },
      });
      if (!response.ok) {
        runtimeCache = null;
        return DEFAULT_RUNTIME;
      }
      runtimeCache = normalizeRuntime(await response.json());
      return runtimeCache;
    } catch {
      runtimeCache = null;
      return DEFAULT_RUNTIME;
    } finally {
      runtimePromise = null;
    }
  })();
  return runtimePromise;
}

function normalizeParams(params?: InvokeParams): Record<string, unknown> {
  return params && typeof params === 'object' ? params : {};
}

async function invokeWebRpc<T>(
  method: string,
  params?: InvokeParams,
): Promise<T> {
  let runtime = await loadRuntimeCapabilities();
  if (runtime.mode !== 'web-gateway') {
    runtime = await loadRuntimeCapabilities(true);
  }
  if (runtime.mode !== 'web-gateway') {
    runtime = {
      ...DEFAULT_RUNTIME,
      mode: 'web-gateway',
    };
  }

  const response = await fetch(runtime.rpcBaseUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id:
        typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
          ? crypto.randomUUID()
          : String(Date.now()),
      method,
      params: normalizeParams(params),
    }),
  });

  let payload: JsonRpcResponse<T>;
  try {
    payload = (await response.json()) as JsonRpcResponse<T>;
  } catch {
    if (!response.ok) {
      throw new Error('Cockpit Tools Web runtime is not available.');
    }
    throw new Error('Cockpit Tools Web RPC returned an invalid response.');
  }
  if (!response.ok || payload.error) {
    const detail =
      payload.error?.data && typeof payload.error.data === 'object'
        ? JSON.stringify(payload.error.data)
        : payload.error?.message || response.statusText;
    throw new Error(detail);
  }
  return payload.result as T;
}

export async function invoke<T = unknown>(
  method: string,
  params?: InvokeParams,
): Promise<T> {
  if (isTauriRuntime()) {
    return await tauriInvoke<T>(method, params || {});
  }
  return await invokeWebRpc<T>(method, params);
}
