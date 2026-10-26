import { test } from "node:test";
import assert from "node:assert/strict";
import { verifyDependencies } from "./lib/architecture.mjs";
const rules = { core: ["feature"], feature: [] };
const packages = () => [
  { name: "core", dependencies: [{ name: "feature" }] },
  { name: "feature", dependencies: [] },
];
test("accepts the permitted orchestration direction", () =>
  verifyDependencies(packages(), rules));
for (const kind of [null, "dev", "build"]) {
  test(`rejects a reverse ${kind || "normal"} dependency`, () => {
    const input = packages();
    input[1].dependencies.push({ name: "core", kind });
    assert.throws(() => verifyDependencies(input, rules), /forbidden/);
  });
}
test("rejects unreviewed members", () =>
  assert.throws(() =>
    verifyDependencies(
      [...packages(), { name: "new", dependencies: [] }],
      rules,
    ),
  ));
test("rejects default test support", () => {
  const input = packages();
  input[0].features = { default: ["test-support"] };
  assert.throws(() => verifyDependencies(input, rules), /opt-in/);
});
test("rejects indirect default test support", () => {
  const input = packages();
  input[0].features = { default: ["helpers"], helpers: ["test-support"] };
  assert.throws(() => verifyDependencies(input, rules), /opt-in/);
});
test("rejects test support enabled by a dependency", () => {
  const input = packages();
  input[0].dependencies[0].features = ["test-support"];
  assert.throws(() => verifyDependencies(input, rules), /test support/);
});
