import { setTimeout as delay } from 'node:timers/promises';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const token = process.env.COCKPIT_TOOLS_RPC_TOKEN;
const serviceAddr = process.env.COCKPIT_TOOLS_SERVICE_ADDR;
const webAddr = process.env.COCKPIT_TOOLS_WEB_ADDR;

if (!token || !serviceAddr || !webAddr) {
  throw new Error('COCKPIT_TOOLS_RPC_TOKEN, COCKPIT_TOOLS_SERVICE_ADDR, and COCKPIT_TOOLS_WEB_ADDR are required');
}

const serviceUrl = `http://${serviceAddr}`;
const webUrl = `http://${webAddr}`;
let eventReader = null;
let eventTextBuffer = '';
const tempDirs = [];
let smokeBackupFileName = null;

async function waitFor(url, init = {}) {
  let lastError;
  for (let i = 0; i < 80; i += 1) {
    try {
      const response = await fetch(url, init);
      if (response.ok) {
        return response;
      }
      lastError = new Error(`${url} returned ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(250);
  }
  throw lastError || new Error(`timed out waiting for ${url}`);
}

async function rpc(url, method, params = {}, headers = {}) {
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...headers,
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: method,
      method,
      params,
    }),
  });
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    throw new Error(`${method} returned non-json HTTP ${response.status}: ${text}`);
  }
  return { response, body };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function readSseUntil(predicate, label, timeoutMs = 7000) {
  const decoder = new TextDecoder();
  const deadline = Date.now() + timeoutMs;
  if (predicate(eventTextBuffer)) {
    return eventTextBuffer;
  }
  while (Date.now() < deadline) {
    const read = await Promise.race([
      eventReader.read(),
      delay(500).then(() => null),
    ]);
    if (!read) {
      continue;
    }
    if (read.done) {
      break;
    }
    eventTextBuffer += decoder.decode(read.value, { stream: true });
    if (predicate(eventTextBuffer)) {
      return eventTextBuffer;
    }
  }
  throw new Error(`timed out waiting for SSE ${label}. received: ${eventTextBuffer.slice(0, 500)}`);
}

try {
  await waitFor(`${serviceUrl}/health`);
  await waitFor(`${webUrl}/api/runtime`);

  const health = await fetch(`${serviceUrl}/health`, {
    headers: { 'x-cockpit-tools-rpc-token': token },
  });
  assert(health.ok, 'service health should be available');

  const rejected = await rpc(
    `${serviceUrl}/rpc`,
    'initialize',
    {},
    { 'x-cockpit-tools-rpc-token': 'wrong-token' },
  );
  assert(rejected.response.status === 401, 'service RPC should reject bad token');
  assert(rejected.body.error === 'rpc_token_required', 'bad token should return rpc_token_required');

  const runtime = await (await fetch(`${webUrl}/api/runtime`)).json();
  assert(runtime.mode === 'web-gateway', 'runtime mode should be web-gateway');
  assert(runtime.rpcBaseUrl === '/api/rpc', 'runtime rpcBaseUrl should point at gateway');
  assert(runtime.features?.serviceHostExecution === true, 'runtime should declare service host execution');

  const generalConfig = await rpc(`${webUrl}/api/rpc`, 'get_general_config');
  assert(!generalConfig.body.error, 'get_general_config should succeed through gateway');
  assert(typeof generalConfig.body.result?.language === 'string', 'general config should include language');
  assert(typeof generalConfig.body.result?.theme === 'string', 'general config should include theme');

  const terminals = await rpc(`${webUrl}/api/rpc`, 'get_available_terminals');
  assert(!terminals.body.error, 'get_available_terminals should succeed through gateway');
  assert(
    Array.isArray(terminals.body.result) && terminals.body.result.includes('system'),
    'terminal list should include system',
  );

  const setAppPath = await rpc(`${webUrl}/api/rpc`, 'set_app_path', {
    app: 'opencode',
    path: generalConfig.body.result.opencode_app_path || '',
  });
  assert(!setAppPath.body.error, 'set_app_path should accept a no-op service-host path update');

  const setCodexLaunchOnSwitch = await rpc(`${webUrl}/api/rpc`, 'set_codex_launch_on_switch', {
    enabled: generalConfig.body.result.codex_launch_on_switch,
  });
  assert(!setCodexLaunchOnSwitch.body.error, 'set_codex_launch_on_switch should accept a no-op update');
  const setCodexLocalAccessEntryVisible = await rpc(
    `${webUrl}/api/rpc`,
    'set_codex_local_access_entry_visible',
    {
      enabled: generalConfig.body.result.codex_local_access_entry_visible,
    },
  );
  assert(
    !setCodexLocalAccessEntryVisible.body.error,
    'set_codex_local_access_entry_visible should accept a no-op update',
  );
  const setFloatingAlwaysOnTop = await rpc(`${webUrl}/api/rpc`, 'set_floating_card_always_on_top', {
    alwaysOnTop: generalConfig.body.result.floating_card_always_on_top,
  });
  assert(!setFloatingAlwaysOnTop.body.error, 'set_floating_card_always_on_top should accept a no-op update');
  const setFloatingConfirmOnClose = await rpc(`${webUrl}/api/rpc`, 'set_floating_card_confirm_on_close', {
    confirmOnClose: generalConfig.body.result.floating_card_confirm_on_close,
  });
  assert(
    !setFloatingConfirmOnClose.body.error,
    'set_floating_card_confirm_on_close should accept a no-op update',
  );
  const closeWindowNoop = await rpc(`${webUrl}/api/rpc`, 'handle_window_close', {
    action: 'minimize',
    remember: false,
  });
  assert(!closeWindowNoop.body.error, 'handle_window_close should validate a web no-op close action');

  const networkConfig = await rpc(`${webUrl}/api/rpc`, 'get_network_config');
  assert(!networkConfig.body.error, 'get_network_config should succeed through gateway');
  assert(typeof networkConfig.body.result?.ws_enabled === 'boolean', 'network config should include ws_enabled');
  assert(typeof networkConfig.body.result?.ws_port === 'number', 'network config should include ws_port');
  assert(
    networkConfig.body.result?.default_port === 19528,
    'network config should include the default websocket port',
  );
  const saveGeneralConfig = await rpc(`${webUrl}/api/rpc`, 'save_general_config', {
    language: generalConfig.body.result.language,
    theme: generalConfig.body.result.theme,
    uiScale: generalConfig.body.result.ui_scale,
  });
  assert(!saveGeneralConfig.body.error, 'save_general_config should accept partial web updates');
  const networkAfterGeneralSave = await rpc(`${webUrl}/api/rpc`, 'get_network_config');
  assert(
    networkAfterGeneralSave.body.result?.ws_enabled === networkConfig.body.result.ws_enabled &&
      networkAfterGeneralSave.body.result?.ws_port === networkConfig.body.result.ws_port &&
      networkAfterGeneralSave.body.result?.report_token === networkConfig.body.result.report_token,
    'save_general_config should not rewrite network settings',
  );
  const saveNetworkConfig = await rpc(`${webUrl}/api/rpc`, 'save_network_config', {
    wsEnabled: networkConfig.body.result.ws_enabled,
    wsPort: networkConfig.body.result.ws_port,
  });
  assert(
    saveNetworkConfig.body.result === false,
    'save_network_config should accept a no-op update without requiring restart',
  );

  const directInit = await rpc(
    `${serviceUrl}/rpc`,
    'initialize',
    {},
    { 'x-cockpit-tools-rpc-token': token },
  );
  assert(directInit.response.ok, 'direct initialize HTTP should succeed');
  assert(directInit.body.result?.service_addr === serviceAddr, 'initialize should report service addr');

  const gatewayInit = await rpc(`${webUrl}/api/rpc`, 'initialize');
  assert(gatewayInit.response.ok, 'gateway initialize HTTP should succeed');
  assert(gatewayInit.body.result?.service_addr === serviceAddr, 'gateway should proxy initialize');

  const unknown = await rpc(`${webUrl}/api/rpc`, 'missing/method');
  assert(unknown.body.error?.message === 'unknown_method', 'unknown RPC should return unknown_method');

  const index = await fetch(`${webUrl}/`);
  const indexText = await index.text();
  assert(index.ok && indexText.includes('<!doctype html>'), 'web gateway should serve dist index.html');

  const events = await fetch(`${webUrl}/api/events`);
  eventReader = events.body.getReader();
  await readSseUntil((text) => text.includes('event: gateway.ready'), 'gateway.ready');
  await readSseUntil((text) => text.includes('event: service.ready'), 'service.ready');

  const externalImport = await fetch(
    `${webUrl}/external-import?provider=codex&token=abc123&auto_import=true&activate=true`,
    { redirect: 'manual' },
  );
  assert(externalImport.status === 302, 'external import should redirect after accepting payload');
  assert(
    externalImport.headers.get('location') === '/?externalImport=1',
    'external import redirect location should mark import state',
  );

  const pending = await rpc(`${webUrl}/api/rpc`, 'external_import_take_pending');
  const payload = pending.body.result;
  assert(payload?.providerId === 'codex', 'pending import should preserve providerId');
  assert(payload?.page === 'codex', 'pending import should preserve page');
  assert(payload?.token === 'abc123', 'pending import should preserve token');
  assert(payload?.autoImport === true, 'pending import should preserve autoImport');
  assert(payload?.activate === true, 'pending import should preserve activate');
  assert(payload?.source === 'web-gateway', 'pending import should mark source');
  assert(
    typeof payload?.rawUrl === 'string' && payload.rawUrl.startsWith('cockpit-tools://import?'),
    'pending import should preserve rawUrl',
  );
  await readSseUntil(
    (text) =>
      text.includes('event: external:provider-import') &&
      text.includes('"providerId":"codex"'),
    'external import event',
  );

  const submit = await rpc(`${webUrl}/api/rpc`, 'external_import_submit_url', {
    rawUrl: 'cockpit-tools://provider-import?platform=codebuddy-cn&payload=%7B%7D&auto_import=1',
    source: 'smoke-test',
  });
  assert(submit.body.result?.providerId === 'codebuddy_cn', 'legacy submit alias should map through catalog');
  await readSseUntil(
    (text) =>
      text.includes('event: external:provider-import') &&
      text.includes('"providerId":"codebuddy_cn"') &&
      text.includes('"source":"smoke-test"'),
    'legacy submit event',
  );

  const corruptDir = await fs.mkdtemp(path.join(os.tmpdir(), 'cockpit-smoke-corrupted-'));
  tempDirs.push(corruptDir);
  const corruptFile = path.join(corruptDir, 'accounts.json');
  await fs.writeFile(corruptFile, 'broken json', 'utf8');
  const deleted = await rpc(`${webUrl}/api/rpc`, 'delete_corrupted_file', {
    path: corruptFile,
  });
  assert(!deleted.body.error, 'corrupted-file delete should succeed through legacy RPC alias');
  await fs.access(corruptFile).then(
    () => {
      throw new Error('corrupted-file delete should remove original path');
    },
    () => undefined,
  );
  const backupNames = (await fs.readdir(corruptDir)).filter((name) =>
    name.startsWith('accounts.json.corrupted.'),
  );
  assert(backupNames.length === 1, 'corrupted-file delete should create one backup file');
  const backupContent = await fs.readFile(path.join(corruptDir, backupNames[0]), 'utf8');
  assert(backupContent === 'broken json', 'corrupted-file backup should preserve content');

  const systemDir = await fs.mkdtemp(path.join(os.tmpdir(), 'cockpit-smoke-system-host-'));
  tempDirs.push(systemDir);
  const textPath = path.join(systemDir, 'service-host.txt');
  const saveText = await rpc(`${webUrl}/api/rpc`, 'save_text_file', {
    path: textPath,
    content: 'service host content',
  });
  assert(!saveText.body.error, 'save_text_file should write on service host');
  const readText = await rpc(`${webUrl}/api/rpc`, 'read_text_file', {
    path: textPath,
  });
  assert(readText.body.result === 'service host content', 'read_text_file should read service host content');
  const home = await rpc(`${webUrl}/api/rpc`, 'get_home_dir');
  assert(typeof home.body.result === 'string' && home.body.result.length > 0, 'get_home_dir should return service host home');
  const downloads = await rpc(`${webUrl}/api/rpc`, 'get_downloads_dir');
  assert(
    typeof downloads.body.result === 'string' && downloads.body.result.length > 0,
    'get_downloads_dir should return service host downloads path',
  );

  const backupFileName = `cockpit_auto_backup_smoke_${Date.now()}.json`;
  smokeBackupFileName = backupFileName;
  const autoBackupContent = JSON.stringify({
    accounts: {
      platforms: {
        codex: { data: [{ id: 'one' }, { id: 'two' }] },
      },
    },
    exported_at: new Date().toISOString(),
  });
  const writeBackup = await rpc(`${webUrl}/api/rpc`, 'write_auto_backup_file', {
    fileName: backupFileName,
    content: autoBackupContent,
  });
  assert(!writeBackup.body.error, 'write_auto_backup_file should succeed through gateway');
  assert(
    typeof writeBackup.body.result === 'string' && writeBackup.body.result.endsWith(backupFileName),
    'write_auto_backup_file should return the service host backup path',
  );
  const readBackup = await rpc(`${webUrl}/api/rpc`, 'read_auto_backup_file', {
    fileName: backupFileName,
  });
  assert(readBackup.body.result === autoBackupContent, 'read_auto_backup_file should round-trip backup JSON');
  const backups = await rpc(`${webUrl}/api/rpc`, 'list_auto_backup_files');
  const backupEntry = backups.body.result?.find?.((entry) => entry.file_name === backupFileName);
  assert(backupEntry, 'list_auto_backup_files should include the smoke backup');
  assert(backupEntry.archive_file_name === backupFileName.replace(/\.json$/, '.zip'), 'backup list should include archive metadata');
  assert(backupEntry.platforms?.[0]?.platform === 'codex', 'backup list should include platform summary');
  assert(backupEntry.platforms?.[0]?.account_count === 2, 'backup platform summary should include account count');
  const webdavSettings = await rpc(`${webUrl}/api/rpc`, 'get_webdav_sync_settings');
  assert(
    typeof webdavSettings.body.result?.remote_dir === 'string',
    'get_webdav_sync_settings should return persisted WebDAV settings',
  );
  const deleteBackup = await rpc(`${webUrl}/api/rpc`, 'delete_auto_backup_file', {
    fileName: backupFileName,
  });
  assert(!deleteBackup.body.error, 'delete_auto_backup_file should succeed through gateway');
  smokeBackupFileName = null;
  const backupsAfterDelete = await rpc(`${webUrl}/api/rpc`, 'list_auto_backup_files');
  assert(
    !backupsAfterDelete.body.result?.some?.((entry) => entry.file_name === backupFileName),
    'delete_auto_backup_file should remove the smoke backup',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        serviceAddr,
        webAddr,
        checks: [
          'health',
          'rpc token rejection',
          'runtime',
          'general config rpc',
          'terminal list rpc',
          'service host app path rpc',
          'small config mutator rpc',
          'desktop shell close noop rpc',
          'network config rpc',
          'direct rpc',
          'gateway rpc',
          'static dist',
          'external import redirect',
          'pending import payload',
          'legacy submit alias',
          'sse forwarding',
          'corrupted file backup via gateway',
          'service host text file rpc',
          'service host path rpc',
          'auto backup rpc',
          'webdav settings rpc',
        ],
      },
      null,
      2,
    ),
  );
} finally {
  if (eventReader) {
    await eventReader.cancel().catch(() => {});
    eventReader = null;
  }
  if (smokeBackupFileName) {
    await rpc(`${webUrl}/api/rpc`, 'delete_auto_backup_file', {
      fileName: smokeBackupFileName,
    }).catch(() => {});
  }
  await Promise.allSettled(
    tempDirs.map((dir) => fs.rm(dir, { recursive: true, force: true })),
  );
}
