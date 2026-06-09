import {
  check as tauriCheck,
  type Update,
} from '@tauri-apps/plugin-updater';
import { isTauriRuntime } from './invoke';

export type { Update };

type CheckOptions = Parameters<typeof tauriCheck>[0];

export async function check(options?: CheckOptions): Promise<Update | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  return tauriCheck(options);
}
