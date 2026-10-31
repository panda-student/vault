# Signs artifacts/latest.json for a stable tag. The signing key comes from the environment.
param([Parameter(Mandatory)][string]$RefName)
$ErrorActionPreference = 'Stop'
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
node scripts/update-release.mjs manifest "artifacts/vault-$version-setup.exe" artifacts/latest.json $RefName "docs/releases/$version.md"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

