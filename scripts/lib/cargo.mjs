import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
const local = join(
  homedir(),
  ".cargo",
  "bin",
  process.platform === "win32" ? "cargo.exe" : "cargo",
);
export const cargo = existsSync(local) ? local : "cargo";
