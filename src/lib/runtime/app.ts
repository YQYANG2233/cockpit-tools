import { getVersion as tauriGetVersion } from '@tauri-apps/api/app';
import packageJson from '../../../package.json';
import { isTauriRuntime, loadRuntimeCapabilities } from './invoke';

export async function getVersion(): Promise<string> {
  if (isTauriRuntime()) {
    return await tauriGetVersion();
  }

  const runtime = await loadRuntimeCapabilities();
  const featureVersion = runtime.features?.appVersion;
  if (typeof featureVersion === 'string' && featureVersion.trim()) {
    return featureVersion;
  }

  return packageJson.version;
}
