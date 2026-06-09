import { relaunch as tauriRelaunch } from '@tauri-apps/plugin-process';
import { isTauriRuntime } from './invoke';

export async function relaunch(): Promise<void> {
  if (isTauriRuntime()) {
    await tauriRelaunch();
    return;
  }
  if (typeof window !== 'undefined') {
    window.location.reload();
  }
}
