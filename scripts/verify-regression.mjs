// Run real isolated fixtures serially: native focus and window tests cannot overlap.
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
const executable = resolve(
  process.env.VAULT_EXE || "target/debug/local-password-manager.exe",
);
const bytes = await readFile(executable);
const report = {
  passed: false,
  executable,
  executableSha256: createHash("sha256").update(bytes).digest("hex"),
  startedAt: new Date().toISOString(),
  suites: [],
};
await mkdir("artifacts/regression", { recursive: true });
const suites = [
  "next",
  "permissions",
  "mcp-handoff",
  "unified-backend",
  "write",
  "browser-fill",
];
for (const suite of suites) {
  if (suite === "write" || suite === "browser-fill") {
    const prerequisite = spawn(
      "powershell.exe",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "scripts/check-interactive-desktop.ps1",
      ],
      { windowsHide: true, stdio: "inherit" },
    );
    const ready = await new Promise((accept, reject) => {
      prerequisite.once("error", reject);
      prerequisite.once("exit", accept);
    });
    if (ready !== 0) {
      report.suites.push({
        suite,
        passed: false,
        exitCode: ready,
        blocked: "interactive-desktop-unavailable",
      });
      process.exitCode = 1;
      break;
    }
  }
  const started = Date.now();
  console.log(`Starting ${suite}`);
  const child = spawn(process.execPath, [`scripts/verify-${suite}.mjs`], {
    windowsHide: true,
    env: { ...process.env, VAULT_EXE: executable, VAULT_WRITE_EXE: executable },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (chunk) => {
      output += chunk;
      process.stdout.write(chunk);
    });
  const timer = setTimeout(() => child.kill(), 360000);
  const exitCode = await new Promise((accept, reject) => {
    child.once("error", reject);
    child.once("exit", accept);
  });
  clearTimeout(timer);
  await writeFile(`artifacts/regression/${suite}.log`, output);
  report.suites.push({
    suite,
    passed: exitCode === 0,
    exitCode,
    durationMs: Date.now() - started,
  });
  if (exitCode !== 0) {
    process.exitCode = 1;
    break;
  }
}
report.passed =
  report.suites.length === suites.length &&
  report.suites.every((s) => s.passed);
report.finishedAt = new Date().toISOString();
await writeFile(
  "artifacts/regression/report.json",
  JSON.stringify(report, null, 2),
);
if (!report.passed) process.exitCode = 1;
