// Separate test binaries from distributable builds.
import { spawnSync, execFileSync } from "node:child_process";
import { mkdirSync, copyFileSync } from "node:fs";
import { resolve } from "node:path";
import { cargo } from "./lib/cargo.mjs";
const release = process.argv.includes("--release");
for (const [command, args] of [
  [
    process.execPath,
    [
      "node_modules/vue-tsc/bin/vue-tsc.js",
      "--noEmit",
      "--noUnusedLocals",
      "--noUnusedParameters",
    ],
  ],
  [
    process.execPath,
    ["node_modules/vite/bin/vite.js", "build", "--mode", "test"],
  ],
  [
    cargo,
    [
      "build",
      "-p",
      "local-password-manager",
      "--locked",
      "--features",
      "test-support,custom-protocol",
      ...(release ? ["--release"] : []),
    ],
  ],
]) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
mkdirSync("artifacts/test-build", { recursive: true });
const target = resolve("artifacts/test-build/vault.exe");
copyFileSync(
  `target/${release ? "release" : "debug"}/local-password-manager.exe`,
  target,
);
const info = JSON.parse(
  execFileSync(target, ["--build-info"], {
    encoding: "utf8",
    windowsHide: true,
  }),
);
if (!info.testSupport) throw new Error("Test support was not compiled");
console.log(`Test-only executable: ${target}`);
