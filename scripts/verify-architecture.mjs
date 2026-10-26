import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { cargo } from "./lib/cargo.mjs";
import { verifyDependencies } from "./lib/architecture.mjs";
const metadata = JSON.parse(
  execFileSync(
    cargo,
    ["metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"],
    { encoding: "utf8", maxBuffer: 8e6 },
  ),
);
verifyDependencies(
  metadata.packages,
  JSON.parse(readFileSync("backend/architecture.json", "utf8")),
);
function files(folder) {
  return readdirSync(folder, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(folder, e.name)) : [join(folder, e.name)],
  );
}
for (const file of files("backend").filter((p) => p.endsWith(".rs"))) {
  const source = readFileSync(file, "utf8");
  assert.ok(
    !source.includes("tauri::"),
    `Backend depends on desktop shell: ${file}`,
  );
  assert.ok(
    !/#\[path\s*=/.test(source),
    `Source must belong to its crate: ${file}`,
  );
}
for (const file of files("frontend").filter((p) => /\.(ts|vue)$/.test(p))) {
  if (file.replaceAll("\\", "/") === "frontend/api/desktop.ts") continue;
  assert.ok(
    !readFileSync(file, "utf8").includes("@tauri-apps/api"),
    `UI bypasses RPC client: ${file}`,
  );
}
for (const file of files("src-tauri/src/commands")) {
  assert.ok(
    !/Operation::|rusqlite|vault_(?:storage|permissions|credentials)::/.test(
      readFileSync(file, "utf8"),
    ),
    `Adapter contains business logic: ${file}`,
  );
}
for (const file of [
  "src-tauri/src/commands/mod.rs",
  "backend/mcp/src/lib.rs",
]) {
  const source = readFileSync(file, "utf8");
  for (const bypass of [
    "vault_storage::",
    "rusqlite::",
    "crypto::",
    ".service.",
    "Operation::ReadField",
    "Operation::EmptyTrash",
    "Operation::SaveSettings",
    ".protected_values(",
  ]) {
    assert.ok(
      !source.includes(bypass),
      `Adapter bypasses core: ${file}: ${bypass}`,
    );
  }
}
for (const file of [
  ...files("backend/filling/src"),
  "backend/contracts/src/filling.rs",
].filter((p) => p.endsWith(".rs"))) {
  const source = readFileSync(file, "utf8");
  for (const native of [
    "Win32::",
    "SendInput",
    "ReplaceFileW",
    "GetForegroundWindow",
    "UIA_",
    'extern "system"',
  ]) {
    assert.ok(
      !source.includes(native),
      `Filling bypasses platform: ${file}: ${native}`,
    );
  }
}
assert.equal(
  Buffer.from(
    readFileSync("backend/updater/update-public-key.txt", "utf8").trim(),
    "base64",
  ).length,
  32,
);
const ci = readFileSync(".github/workflows/ci-build.yml", "utf8");
assert.ok(
  ci.includes("node scripts/preflight.mjs --ci"),
  "CI must run the shared preflight",
);
const pipeline = readFileSync("scripts/preflight.mjs", "utf8");
for (const required of [
  "verify:architecture",
  "Rust tests",
  "scripts/build-test.mjs",
  "desktop:build",
  "scripts/verify-production.mjs",
  "scripts/package-installer.ps1",
  "scripts/verify-installer.mjs",
]) {
  assert.ok(
    pipeline.includes(required),
    `Shared preflight is missing ${required}`,
  );
}
execFileSync(process.execPath, ["--test", "scripts/architecture.test.mjs"], {
  stdio: "inherit",
});
console.log(
  "PASS workspace dependency allowlist, source ownership and adapter boundaries",
);
