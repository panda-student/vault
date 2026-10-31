// The same fail-fast build/package/installation pipeline runs locally and in CI.
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { cargo } from "./lib/cargo.mjs";
import { sourceFingerprint } from "./lib/source-fingerprint.mjs";
const ci = process.argv.includes("--ci");
const id = randomUUID();
const artifacts = resolve(ci ? "artifacts" : `artifacts/preflight/${id}`);
mkdirSync(artifacts, { recursive: true });
mkdirSync("artifacts/preflight", { recursive: true });
const report = {
  passed: false,
  startedAt: new Date().toISOString(),
  sourceFingerprint: sourceFingerprint(),
  artifacts,
  steps: [],
};
const npmCli =
  process.env.npm_execpath ||
  join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js");
function run(name, command, args, env = {}, output) {
  console.log(`\n== ${name} ==`);
  const result = spawnSync(command, args, {
    windowsHide: true,
    env: { ...process.env, ...env },
    stdio: output ? ["ignore", "pipe", "inherit"] : "inherit",
    maxBuffer: 32 * 1024 * 1024,
  });
  report.steps.push({ name, passed: !result.error && result.status === 0 });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw Error(`${name} failed with exit code ${result.status}`);
  if (output) writeFileSync(output, result.stdout);
}
const node = (name, script, env = {}) =>
  run(name, process.execPath, [script], env);
const npm = (name, script) =>
  run(name, process.execPath, [npmCli, "run", script]);
const ps = (name, script, args = []) =>
  run(name, "powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    script,
    ...args,
  ]);
try {
  run("Locked frontend dependencies", process.execPath, [npmCli, "ci"]);
  run("Validate versions", process.execPath, [
    "scripts/update-release.mjs",
    "validate",
  ]);
  npm("Architecture boundaries", "verify:architecture");
  npm("Frontend formatting", "format:check");
  run("Rust formatting", cargo, ["fmt", "--all", "--check"]);
  npm("Frontend build", "build");
  run("Rust tests", cargo, ["test", "--workspace", "--locked"]);
  mkdirSync("target", { recursive: true });
  run(
    "Dependency graph",
    cargo,
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      "x86_64-pc-windows-msvc",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ],
    {},
    "target/dependency-metadata.json",
  );
  node("Dependency notices", "scripts/generate-notices.mjs");
  run("Test-only release build", process.execPath, [
    "scripts/build-test.mjs",
    "--release",
  ]);
  for (const name of ["next", "permissions", "mcp-handoff", "quit-recovery"])
    node(name, `scripts/verify-${name}.mjs`, {
      VAULT_EXE: resolve("artifacts/test-build/vault.exe"),
    });
  npm("Production release build", "desktop:build");
  node("Production isolation", "scripts/verify-production.mjs", {
    VAULT_EXE: resolve("target/release/local-password-manager.exe"),
  });
  ps("Production installer", "scripts/package-installer.ps1", [
    "-ArtifactDirectory",
    artifacts,
  ]);
  const version = JSON.parse(readFileSync("package.json", "utf8")).version;
  // A developer may have vault installed already. Use a different registration
  // namespace for local acceptance, with identical payload and installer code.
  const registration = ci ? "vault" : `vault-acceptance-${id}`;
  const acceptance = ci ? artifacts : join(artifacts, "acceptance");
  if (!ci) {
    mkdirSync(acceptance, { recursive: true });
    ps("Isolated acceptance installer", "scripts/package-installer.ps1", [
      "-ArtifactDirectory",
      acceptance,
      "-RegistrationName",
      registration,
    ]);
  }
  node(
    "Install, overwrite and uninstall preservation",
    "scripts/verify-installer.mjs",
    { VAULT_ARTIFACTS: acceptance, VAULT_INSTALLER_REG_NAME: registration },
  );
  for (const mode of ["handoff", "rollback", "drain"]) {
    node(`Installer ${mode}`, "scripts/verify-installer-handoff.mjs", {
      VAULT_HELPER_EXE: join(artifacts, `vault-${version}/vault.exe`),
      VAULT_OLD_EXE: join(artifacts, `vault-${version}/vault.exe`),
      VAULT_INSTALLER: join(acceptance, `vault-${version}-setup.exe`),
      VAULT_INSTALLER_REG_NAME: registration,
      VAULT_TEST_ROLLBACK: mode === "rollback" ? "1" : "0",
      VAULT_TEST_DRAIN: mode === "drain" ? "1" : "0",
      VAULT_HANDOFF_REPORT: join(
        artifacts,
        `installer-${mode}-verification.json`,
      ),
    });
  }
  if (report.sourceFingerprint !== sourceFingerprint())
    throw Error("Source changed during preflight; rerun on the final source");
  report.passed = true;
} catch (error) {
  report.error = error.message;
  console.error(error);
  process.exitCode = 1;
} finally {
  report.finishedAt = new Date().toISOString();
  writeFileSync(
    join(artifacts, "preflight.json"),
    JSON.stringify(report, null, 2),
  );
  writeFileSync(
    "artifacts/preflight/latest.json",
    JSON.stringify(report, null, 2),
  );
}
