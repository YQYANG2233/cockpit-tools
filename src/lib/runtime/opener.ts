import {
  openPath as tauriOpenPath,
  openUrl as tauriOpenUrl,
} from '@tauri-apps/plugin-opener';
import { invoke, isTauriRuntime } from './invoke';

export async function openUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await tauriOpenUrl(url);
    return;
  }

  if (typeof window !== 'undefined') {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

export async function openPath(path: string): Promise<void> {
  if (isTauriRuntime()) {
    await tauriOpenPath(path);
    return;
  }

  await invoke('open_path', { path });
}
