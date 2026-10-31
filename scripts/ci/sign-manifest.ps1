# Signs artifacts/latest.json for the version in package.json. The signing key comes from the environment.
$ErrorActionPreference = 'Stop'
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
node scripts/update-release.mjs manifest "artifacts/vault-$version-setup.exe" artifacts/latest.json "v$version" "docs/releases/$version.md"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
