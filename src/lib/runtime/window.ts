import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWebview as tauriGetCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow as tauriGetCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from './invoke';

export { LogicalSize };

type DesktopWindowFallback = {
  label: string;
  startDragging: () => Promise<void>;
  setSize: (_size: LogicalSize) => Promise<void>;
  onCloseRequested: (_handler: (event: unknown) => void) => Promise<() => void>;
  onMoved: (_handler: (event: { payload: unknown }) => void) => Promise<() => void>;
};

type DesktopWebviewFallback = {
  setZoom: (_scale: number) => Promise<void>;
};

const WEB_WINDOW: DesktopWindowFallback = {
  label: 'web',
  startDragging: async () => {},
  setSize: async () => {},
  onCloseRequested: async () => () => {},
  onMoved: async () => () => {},
};

const WEB_WEBVIEW: DesktopWebviewFallback = {
  setZoom: async () => {},
};

export function getCurrentWindow(): ReturnType<typeof tauriGetCurrentWindow> {
  return (isTauriRuntime() ? tauriGetCurrentWindow() : WEB_WINDOW) as ReturnType<
    typeof tauriGetCurrentWindow
  >;
}

export function getCurrentWebview(): ReturnType<typeof tauriGetCurrentWebview> {
  return (isTauriRuntime() ? tauriGetCurrentWebview() : WEB_WEBVIEW) as ReturnType<
    typeof tauriGetCurrentWebview
  >;
}
