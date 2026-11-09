// One fail-fast pipeline shared by local runs and CI. CI runs each phase as its own job.
import { spawnSync } from "node:child_process";
import {
  appendFileSync,
  mkdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { cargo } from "./lib/cargo.mjs";
import { sourceFingerprint } from "./lib/source-fingerprint.mjs";
const PHASES = ["checks", "verify", "build"];
const ci = process.argv.includes("--ci");
const phaseArg = process.argv.find((arg) => arg.startsWith("--phase="));
const requested = phaseArg ? phaseArg.slice("--phase=".length) : null;
if (requested && !PHASES.includes(requested))
  throw Error(`Unknown preflight phase: ${requested}`);
const phases = requested ? [requested] : PHASES;
const id = randomUUID();
const artifacts = resolve(ci ? "artifacts" : `artifacts/preflight/${id}`);
mkdirSync(artifacts, { recursive: true });
const npmCli =
  process.env.npm_execpath ||
  join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js");
const vite = "node_modules/vite/bin/vite.js";
let report = null;
function run(name, command, args, env = {}, output) {
  console.log(`\n== ${name} ==`);
  const started = Date.now();
  const result = spawnSync(command, args, {
    windowsHide: true,
    env: { ...process.env, ...env },
    stdio: output ? ["ignore", "pipe", "inherit"] : "inherit",
    maxBuffer: 32 * 1024 * 1024,
  });
  const passed = !result.error && result.status === 0;
  report.steps.push({ name, passed, durationMs: Date.now() - started });
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
const installDependencies = () =>
  run("Locked frontend dependencies", process.execPath, [npmCli, "ci"]);
function generateDependencyNotices() {
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
}
function checks() {
  installDependencies();
  run("Validate versions", process.execPath, [
    "scripts/update-release.mjs",
    "validate",
  ]);
  npm("Architecture boundaries", "verify:architecture");
  npm("Frontend formatting", "format:check");
  run("Rust formatting", cargo, ["fmt", "--all", "--check"]);
  npm("Frontend type check", "typecheck");
  run("Frontend bundle", process.execPath, [vite, "build"]);
  run("Rust tests", cargo, ["test", "--workspace", "--locked"]);
  generateDependencyNotices();
}
// The integration checks exercise behavior, so the test executable uses the
// ci-test profile (release without whole-program LTO) in its own target
// directory. The type check already ran in `checks`.
function verify() {
  installDependencies();
  run("Test-only build", process.execPath, [
    "scripts/build-test.mjs",
    "--profile=ci-test",
    "--skip-typecheck",
  ]);
  for (const name of ["next", "permissions", "mcp-handoff", "quit-recovery"])
    node(name, `scripts/verify-${name}.mjs`, {
      VAULT_EXE: resolve("artifacts/test-build/vault.exe"),
    });
}
function build() {
  installDependencies();
  run(
    "Frontend production bundle",
    process.execPath,
    [vite, "build"],
  );
  // Same executable the Tauri CLI builds with --no-bundle, minus the duplicate
  // frontend build that its beforeBuildCommand would run.
  run("Production release build", cargo, [
    "build",
    "-p",
    "vault",
    "--locked",
    "--release",
    "--features",
    "custom-protocol",
  ]);
  node("Production isolation", "scripts/verify-production.mjs", {
    VAULT_EXE: resolve("target/release/vault.exe"),
  });
  generateDependencyNotices();
  ps("Production installer", "scripts/package-installer.ps1", [
    "-ArtifactDirectory",
    artifacts,
  ]);
}
const handlers = { checks, verify, build };
// Writes a per-phase timing report, and a table on the CI run summary page.
function publish(current) {
  const seconds = (ms) => (ms / 1000).toFixed(1);
  writeFileSync(
    join(artifacts, `preflight-${current.phase}.json`),
    JSON.stringify(current, null, 2),
  );
  if (!process.env.GITHUB_STEP_SUMMARY) return;
  const rows = current.steps.map(
    (step) =>
      `| ${step.name} | ${step.passed ? "通过" : "失败"} | ${seconds(step.durationMs)} 秒 |`,
  );
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    [
      `### 预检：${current.phase}（共 ${seconds(current.durationMs)} 秒）`,
      "",
      "| 步骤 | 结果 | 耗时 |",
      "| --- | --- | --- |",
      ...rows,
      "",
    ].join("\n"),
  );
}
let exitCode = 0;
for (const phase of phases) {
  const started = Date.now();
  report = {
    phase,
    passed: false,
    startedAt: new Date().toISOString(),
    sourceFingerprint: sourceFingerprint(),
    artifacts,
    steps: [],
  };
  try {
    handlers[phase]();
    if (report.sourceFingerprint !== sourceFingerprint())
      throw Error("Source changed during preflight; rerun on the final source");
    report.passed = true;
  } catch (error) {
    report.error = error.message;
    console.error(error);
    exitCode = 1;
  } finally {
    report.durationMs = Date.now() - started;
    report.finishedAt = new Date().toISOString();
    publish(report);
  }
  if (exitCode) break;
}
process.exitCode = exitCode;
