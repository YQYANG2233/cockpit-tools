import {
  readTextFile as tauriReadTextFile,
  writeTextFile as tauriWriteTextFile,
} from '@tauri-apps/plugin-fs';
import { invoke, isTauriRuntime } from './invoke';

export async function writeTextFile(path: string, contents: string): Promise<void> {
  if (isTauriRuntime()) {
    await tauriWriteTextFile(path, contents);
    return;
  }
  await invoke('save_text_file', { path, content: contents });
}

export async function readTextFile(path: string): Promise<string> {
  if (isTauriRuntime()) {
    return tauriReadTextFile(path);
  }
  return invoke<string>('read_text_file', { path });
}
