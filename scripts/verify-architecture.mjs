import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
async function files(folder) {
  const result = [];
  for (const e of await readdir(folder, { withFileTypes: true })) {
    const p = join(folder, e.name);
    if (e.isDirectory()) result.push(...(await files(p)));
    else if (/\.(rs|ts|vue)$/.test(p)) result.push(p);
  }
  return result;
}
const rules = [
  [
    "backend/src/definitions",
    /crate::(?:services|data|platform|adapters)|tauri::/,
  ],
  [
    "backend/src/services",
    /crate::(?:api|data|platform)|tauri::|rusqlite::|windows::/,
  ],
  ["backend/src/data", /crate::(?:services|adapters|platform)|tauri::/],
  ["backend/src/platform", /crate::(?:services|data|adapters)|tauri::/],
  [
    "src-tauri/src/commands",
    /rusqlite|Operation::|vault_backend::(?:data|services|platform)/,
  ],
];
let count = 0;
for (const [folder, forbidden] of rules)
  for (const file of await files(folder)) {
    assert.ok(
      !forbidden.test(await readFile(file, "utf8")),
      `Layer violation: ${file}`,
    );
    count++;
  }
for (const file of await files("frontend")) {
  if (file.replaceAll("\\", "/") === "frontend/api/desktop.ts") continue;
  assert.ok(
    !(await readFile(file, "utf8")).includes("@tauri-apps/api"),
    `UI must use the RPC client: ${file}`,
  );
  count++;
}
assert.ok(!(await readFile("backend/Cargo.toml", "utf8")).includes("tauri"));
console.log(
  `PASS ${count} source files respect interface, service, data and platform boundaries`,
);
