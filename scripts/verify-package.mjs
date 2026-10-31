import {readFile,writeFile,readdir} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const run=promisify(execFile),version=JSON.parse(await readFile('package.json','utf8')).version;
const artifacts=resolve(process.env.VAULT_ARTIFACTS||'artifacts');
// The installer embeds this staging folder; it is verified in place, never published.
const folder=join(artifacts,`vault-${version}`),executable=join(folder,'vault.exe');
const hash=async p=>createHash('sha256').update(await readFile(p)).digest('hex');
const installer=join(artifacts,`vault-${version}-setup.exe`);
assert.equal(JSON.parse((await readFile(join(artifacts,'installer-size.json'),'utf8')).replace(/^﻿/,'')).sha256,await hash(installer));
assert.deepEqual((await readdir(folder)).sort(),['LICENSE.txt','NOTICE.txt','README.txt','THIRD_PARTY_NOTICES.txt','vault.exe','third-party-sources'].sort());
assert.ok((await readFile(join(folder,'LICENSE.txt'),'utf8')).includes('Apache License'));
for(const script of ['verify-production.mjs','verify-next.mjs','verify-write.mjs']){
 const {stdout}=await run(process.execPath,[join('scripts',script)],{windowsHide:true,timeout:240000,maxBuffer:1024*1024,env:{...process.env,VAULT_EXE:executable,VAULT_WRITE_EXE:executable,VAULT_MCP_ONLY:'1'}});
 console.log(stdout);
}
await writeFile('docs/evidence/package-verification.json',JSON.stringify({passed:true,version,executableSha256:await hash(executable),installerSha256:await hash(installer),onlyDistributionFiles:true},null,2));
console.log('PASS installer staging contents, hashes, MCP, desktop, native editors and file writes');
