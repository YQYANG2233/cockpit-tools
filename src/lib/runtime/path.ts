import {
  homeDir as tauriHomeDir,
  join as tauriJoin,
} from '@tauri-apps/api/path';
import { invoke, isTauriRuntime } from './invoke';

export async function homeDir(): Promise<string> {
  if (isTauriRuntime()) {
    return tauriHomeDir();
  }
  return invoke<string>('get_home_dir');
}

function joinServiceHostPath(parts: string[]): string {
  const normalized = parts.filter((part) => part.trim().length > 0);
  if (normalized.length === 0) {
    return '';
  }
  const separator = normalized[0].includes('\\') ? '\\' : '/';
  return normalized
    .map((part, index) => {
      if (index === 0) {
        return part.replace(/[\\/]+$/g, '');
      }
      return part.replace(/^[\\/]+/g, '').replace(/[\\/]+$/g, '');
    })
    .join(separator);
}

export async function join(...paths: string[]): Promise<string> {
  if (isTauriRuntime()) {
    return tauriJoin(...paths);
  }
  return joinServiceHostPath(paths);
}
