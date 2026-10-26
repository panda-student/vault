// Windows browser daemons may inherit stdout handles. Use files so a completed
// CLI command never waits for a long-lived daemon to close a capture pipe.
import { spawn, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  openSync,
  closeSync,
  readFileSync,
  unlinkSync,
  rmdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
function capture() {
  const directory = mkdtempSync(join(tmpdir(), "vault-browser-cli-"));
  const out = join(directory, "stdout"),
    err = join(directory, "stderr");
  const stdout = openSync(out, "w"),
    stderr = openSync(err, "w");
  return {
    stdio: ["ignore", stdout, stderr],
    finish() {
      closeSync(stdout);
      closeSync(stderr);
      const result = {
        stdout: readFileSync(out, "utf8"),
        stderr: readFileSync(err, "utf8"),
      };
      try {
        unlinkSync(out);
        unlinkSync(err);
        rmdirSync(directory);
      } catch {}
      return result;
    },
  };
}
function result(capture, code, error) {
  const output = capture.finish();
  if (error || code !== 0)
    throw Object.assign(
      error ?? new Error(output.stderr || `Browser CLI exited ${code}`),
      output,
    );
  return output;
}
export function runBrowserSync(binary, args, options = {}) {
  const io = capture();
  const child = spawnSync(binary, args, {
    ...options,
    windowsHide: true,
    stdio: io.stdio,
  });
  return result(io, child.status, child.error).stdout;
}
async function capturedBrowser(binary, args, options = {}) {
  const io = capture();
  let child;
  try {
    child = spawn(binary, args, {
      ...options,
      windowsHide: true,
      stdio: io.stdio,
    });
  } catch (error) {
    return result(io, null, error);
  }
  let timer, code, error;
  try {
    code = await new Promise((accept, reject) => {
      child.once("error", reject);
      child.once("exit", accept);
      timer = setTimeout(() => {
        child.kill();
        reject(new Error("Browser CLI timeout"));
      }, options.timeout ?? 30000);
    });
  } catch (failure) {
    error = failure;
  } finally {
    clearTimeout(timer);
  }
  return result(io, code, error);
}

/** This startup error is emitted before a command runs, so one retry is safe. */
export async function runBrowser(binary, args, options = {}) {
  try {
    return await capturedBrowser(binary, args, options);
  } catch (error) {
    if (!String(error.stderr).includes("different daemon configuration"))
      throw error;
    await new Promise((resolve) => setTimeout(resolve, 500));
    return capturedBrowser(binary, args, options);
  }
}
