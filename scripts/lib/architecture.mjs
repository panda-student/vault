import assert from "node:assert/strict";
// Cargo includes normal, dev, build and target-specific dependencies here.
export function verifyDependencies(packages, rules) {
  assert.deepEqual(
    packages.map((p) => p.name).sort(),
    Object.keys(rules).sort(),
    "Workspace membership must be reviewed",
  );
  for (const pkg of packages) {
    for (const dependency of pkg.dependencies) {
      if (
        [
          "vault-contracts",
          "vault-credentials",
          "vault-permissions",
          "vault-filling",
        ].includes(pkg.name)
      ) {
        assert.ok(
          !/^(?:tauri|rusqlite|windows|winreg)(?:-|$)/.test(dependency.name),
          `${pkg.name} must depend on ports, not ${dependency.name}`,
        );
      }
      if (dependency.path || Object.hasOwn(rules, dependency.name)) {
        assert.ok(
          rules[pkg.name].includes(dependency.name),
          `${pkg.name} -> ${dependency.name} (${dependency.kind || "normal"}) is forbidden`,
        );
      }
      assert.ok(
        !dependency.features?.includes("test-support"),
        "Production dependency enables test support",
      );
    }
    const visited = new Set();
    const pending = [...(pkg.features?.default || [])];
    while (pending.length) {
      const feature = pending.pop();
      if (visited.has(feature)) continue;
      visited.add(feature);
      assert.ok(
        !feature.includes("test-support"),
        "Test support must be opt-in",
      );
      pending.push(...(pkg.features?.[feature] || []));
    }
  }
}
