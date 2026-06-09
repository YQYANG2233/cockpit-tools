import {
  confirm as tauriConfirm,
  open as tauriOpen,
  save as tauriSave,
  type ConfirmDialogOptions,
  type OpenDialogOptions,
  type SaveDialogOptions,
} from '@tauri-apps/plugin-dialog';
import { isTauriRuntime } from './invoke';

export type { ConfirmDialogOptions, OpenDialogOptions, SaveDialogOptions };

export async function confirm(
  message: string,
  options?: ConfirmDialogOptions,
): Promise<boolean> {
  if (isTauriRuntime()) {
    return tauriConfirm(message, options);
  }
  return typeof window !== 'undefined' ? window.confirm(message) : false;
}

export async function open(
  options?: OpenDialogOptions,
): Promise<string | string[] | null> {
  if (isTauriRuntime()) {
    return tauriOpen(options);
  }
  console.warn('[runtime/dialog] File selection is not available in web runtime.', options);
  return null;
}

export async function save(options?: SaveDialogOptions): Promise<string | null> {
  if (isTauriRuntime()) {
    return tauriSave(options);
  }
  console.warn('[runtime/dialog] Save dialog is not available in web runtime.', options);
  return null;
}
