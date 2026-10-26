import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { sourceFingerprint } from "./lib/source-fingerprint.mjs";
try {
  const report = JSON.parse(
    readFileSync("artifacts/preflight/latest.json", "utf8"),
  );
  assert.equal(report.passed, true);
  assert.equal(report.sourceFingerprint, sourceFingerprint());
  console.log("PASS current source completed the full local release preflight");
} catch {
  console.error(
    "Push blocked: run npm run verify:prepush successfully for the current source first.",
  );
  process.exitCode = 1;
}
