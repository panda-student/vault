import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
const executable = resolve(
  process.env.VAULT_EXE || "target/release/local-password-manager.exe",
);
const info = JSON.parse(
  execFileSync(executable, ["--build-info"], {
    encoding: "utf8",
    windowsHide: true,
  }),
);
assert.equal(
  info.testSupport,
  false,
  "A test-support executable must not be distributed",
);
const data = mkdtempSync(join(tmpdir(), "vault-production-check-"));
for (const flag of ["--smoke-test", "--fill-fixture"]) {
  const result = spawnSync(
    executable,
    ["--mcp", flag, join(data, "report.json"), "--data-dir", data],
    { timeout: 15000, windowsHide: true },
  );
  assert.equal(result.status, 1, `Production must reject ${flag}`);
}
assert.deepEqual(
  readdirSync(data),
  [],
  "Rejected test commands must not open a vault",
);
console.log("PASS production build excludes test commands");
