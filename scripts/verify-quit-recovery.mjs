// After a tray quit the connection bridge stays alive and relaunches the backend on the next request.
// Runs an isolated copy of VAULT_EXE, so the installed vault and its MCP clients are never touched.
import { mkdtemp, copyFile, mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { realpathSync } from 'node:fs';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';


const root = await mkdtemp(join(tmpdir(), 'vault-quit-recovery-'));
const exe = join(root, 'vault.exe');
await copyFile(process.env.VAULT_EXE || 'artifacts/test-build/vault.exe', exe);
const data = join(root, 'data');
const coordination = join(process.env.LOCALAPPDATA, 'LocalPasswordManager', 'updates',
  createHash('sha256').update(('\\\\?\\' + realpathSync.native(exe)).toLowerCase()).digest('hex'));
const ps = (script) => execFileSync('powershell.exe', ['-NoProfile', '-Command', script], { encoding: 'utf8', windowsHide: true }).trim();
const backendPids = () => ps(`@(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '${exe}' -and $_.CommandLine -match '--backend' } | Select-Object -ExpandProperty ProcessId) -join ','`).split(',').filter(Boolean);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok) => { results.push({ name, ok: !!ok }); console.log(ok ? 'PASS' : 'FAIL', name); };

const client = new Client({ name: 'quit-recovery', version: '1' });
const transport = new StdioClientTransport({ command: exe, args: ['--mcp', '--data-dir', data], stderr: 'pipe' });
transport.stderr?.resume();
try {
  await client.connect(transport);
  const first = await client.listTools();
  check('tools-listed-before-quit', first.tools.length > 0);
  const before = backendPids();
  check('one-backend-running', before.length === 1);
  const bridgePid = transport.pid;

  // Same sequence as the tray: write the quit marker, then the backend exits.
  await mkdir(coordination, { recursive: true });
  await writeFile(join(coordination, 'quit'), String(Math.floor(Date.now() / 1000)));
  ps(`Stop-Process -Id ${before[0]} -Force`);
  await sleep(2500);
  check('backend-not-relaunched-eagerly', backendPids().length === 0);
  check('bridge-process-still-alive', ps(`(Get-Process -Id ${bridgePid} -ErrorAction SilentlyContinue) -ne $null`) === 'True');

  const second = await client.listTools();
  check('next-request-succeeds-without-client-restart', second.tools.length === first.tools.length);
  const after = backendPids();
  check('backend-relaunched-on-demand', after.length === 1 && after[0] !== before[0]);
  check('same-client-connection', transport.pid === bridgePid);
} catch (error) {
  console.log('ERROR', String(error));
  results.push({ name: 'no-exception', ok: false });
} finally {
  await client.close().catch(() => {});
  ps(`Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '${exe}' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`);
}
console.log(JSON.stringify({ passed: results.every((r) => r.ok), checks: results.length }));
process.exit(results.every((r) => r.ok) ? 0 : 1);
