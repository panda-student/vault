// Separate test binaries from distributable builds.
import { spawnSync, execFileSync } from "node:child_process";
import { mkdirSync, copyFileSync } from "node:fs";
import { resolve } from "node:path";
import { cargo } from "./lib/cargo.mjs";
// --release builds the shipping profile; --profile=<name> picks a Cargo profile.
const profileArg = process.argv.find((arg) => arg.startsWith("--profile="));
const profile = profileArg
  ? profileArg.slice("--profile=".length)
  : process.argv.includes("--release")
    ? "release"
    : "dev";
// Preflight type checks once in its own job, so its build can skip the repeat.
const skipTypecheck = process.argv.includes("--skip-typecheck");
for (const [command, args] of [
  ...(skipTypecheck
    ? []
    : [
        [
          process.execPath,
          [
            "node_modules/vue-tsc/bin/vue-tsc.js",
            "--noEmit",
            "--noUnusedLocals",
            "--noUnusedParameters",
          ],
        ],
      ]),
  [
    process.execPath,
    ["node_modules/vite/bin/vite.js", "build", "--mode", "test"],
  ],
  [
    cargo,
    [
      "build",
      "-p",
      "vault",
      "--locked",
      "--features",
      "test-support,custom-protocol",
      ...(profile === "dev" ? [] : ["--profile", profile]),
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
  `target/${profile === "dev" ? "debug" : profile}/vault.exe`,
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
