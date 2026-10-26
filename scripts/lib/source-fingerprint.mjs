import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
export function sourceFingerprint() {
  const files = execFileSync(
    "git",
    ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    { encoding: "utf8" },
  ).split("\0");
  const hash = createHash("sha256");
  for (const file of [...new Set(files)]
    .filter((f) => f && existsSync(f))
    .sort()) {
    // Documentation does not change executable/package behavior.
    if (file.endsWith(".md") || file === "THIRD_PARTY_NOTICES.txt") continue;
    hash.update(file).update("\0").update(readFileSync(file)).update("\0");
  }
  return hash.digest("hex");
}
