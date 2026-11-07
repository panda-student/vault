// Signed, version-bound update metadata. Private keys are never written to the repository.
import { execFileSync } from 'node:child_process';
import { generateKeyPairSync, createPrivateKey, createPublicKey, sign, verify, createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import assert from 'node:assert/strict';

const [command, ...args] = process.argv.slice(2);

const readJson = async (path) => JSON.parse(await readFile(path, 'utf8'));
const packageVersion = async () => (await readJson('package.json')).version;

function workspacePackages() {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], { encoding: 'utf8' }));
  return metadata.packages;
}

const inheritsWorkspaceVersion = (manifest) =>
  /^version\s*\.\s*workspace\s*=\s*true\b/m.test(manifest) ||
  /^version\s*=\s*\{\s*workspace\s*=\s*true\s*\}/m.test(manifest);

async function tauriVersion() {
  const configPath = 'src-tauri/tauri.conf.json';
  const config = await readJson(configPath);
  if (typeof config.version !== 'string') return config.version;
  // Tauri treats an existing path as a package.json and otherwise as a semver.
  const path = resolve(dirname(configPath), config.version);
  if (existsSync(path)) return (await readJson(path)).version;
  if (/[\\/]/.test(config.version) || config.version.endsWith('.json')) {
    throw new Error(`tauri.conf.json version path does not exist: ${path}`);
  }
  return config.version;
}

async function assertVersions(version) {
  for (const pkg of workspacePackages()) {
    assert.equal(pkg.version, version, `${pkg.name} must resolve to ${version}`);
    const manifest = await readFile(pkg.manifest_path, 'utf8');
    assert.ok(inheritsWorkspaceVersion(manifest), `${pkg.name} must inherit the [workspace.package] version`);
  }
  assert.equal(await tauriVersion(), version, 'tauri.conf.json version');
}

if (command === 'validate') {
  const version = await packageVersion();
  await assertVersions(version);
  if (process.env.GITHUB_REF?.startsWith('refs/tags/v')) {
    assert.match(version, /^\d+\.\d+\.\d+$/, 'Stable release tags require a stable version');
    assert.equal(process.env.GITHUB_REF_NAME, `v${version}`);
  }
  assert.equal(Buffer.from((await readFile('backend/updater/update-public-key.txt','utf8')).trim(),'base64').length,32);
} else if (command === 'keygen') {
  const [publicPath, privatePath] = args.map(p => resolve(p));
  if (!publicPath || !privatePath) throw Error('keygen <public-file> <private-file>');
  const keys = generateKeyPairSync('ed25519');
  await mkdir(dirname(privatePath), { recursive: true });
  await writeFile(privatePath, keys.privateKey.export({ format: 'der', type: 'pkcs8' }).toString('base64'), { flag: 'wx', mode: 0o600 });
  const raw = keys.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32);
  await writeFile(publicPath, raw.toString('base64') + '\n', { flag: 'wx' });
} else if (command === 'manifest') {
  const [installer, output, tag, notesFile] = args;
  const version = await packageVersion();
  assert.match(version, /^\d+\.\d+\.\d+$/, 'Only stable versions enter the update feed');
  assert.equal(tag, `v${version}`, 'Tag and package version must match');
  await assertVersions(version);
  const bytes = await readFile(installer);
  const privateKey = createPrivateKey({ key: Buffer.from(process.env.VAULT_UPDATE_SIGNING_KEY || '', 'base64'), format: 'der', type: 'pkcs8' });
  const raw = (await readFile('backend/updater/update-public-key.txt', 'utf8')).trim();
  const expected = createPublicKey(privateKey).export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
  assert.equal(expected, raw, 'Signing key does not match the embedded public key');
  const payload = Buffer.from(JSON.stringify({ schema: 3, version, platform: 'windows-x86_64', url: `https://github.com/panda-student/vault/releases/download/${tag}/vault-${version}-setup.exe`, sha256: createHash('sha256').update(bytes).digest('hex'), size: bytes.length, notes: notesFile ? (await readFile(notesFile, 'utf8')).slice(0, 16000) : `vault ${version}` }));
  const signature = sign(null, payload, privateKey);
  assert.ok(verify(null, payload, createPublicKey(privateKey), signature));
  await writeFile(output, JSON.stringify({ payload: payload.toString('base64'), signature: signature.toString('base64') }, null, 2) + '\n');
} else {
  throw Error('Expected keygen or manifest');
}
