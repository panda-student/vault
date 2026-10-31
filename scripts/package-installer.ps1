param([string]$ArtifactDirectory, [ValidatePattern('^[a-z0-9-]+$')][string]$RegistrationName='vault')
$ErrorActionPreference = 'Stop'
function Get-Sha256([string]$Path) {
    $stream=[IO.File]::OpenRead($Path)
    $hasher=[Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-','').ToLowerInvariant() }
    finally { $stream.Dispose();$hasher.Dispose() }
}
$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseBinary = Join-Path $projectRoot 'target\release\local-password-manager.exe'
if (!(Test-Path -LiteralPath $releaseBinary)) { throw 'Run npm run desktop:build first.' }
$previousExe = $env:VAULT_EXE
try {
    $env:VAULT_EXE = $releaseBinary
    & node (Join-Path $PSScriptRoot 'verify-production.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'Refusing to package a test-support build.' }
} finally { $env:VAULT_EXE = $previousExe }
$artifactRoot = if ($ArtifactDirectory) { [IO.Path]::GetFullPath($ArtifactDirectory) } else { Join-Path $projectRoot 'artifacts' }
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
# Staging holds only the files the installer embeds. It is never published.
$package = Join-Path $artifactRoot "vault-$version"
New-Item -ItemType Directory -Path $package -Force | Out-Null
if (@(Get-ChildItem -LiteralPath $package -Force).Count -gt 0) { throw 'Staging directory is not empty. Move it aside before repackaging.' }
Copy-Item -LiteralPath $releaseBinary -Destination (Join-Path $package 'vault.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\INSTALL-README.txt') -Destination (Join-Path $package 'README.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination (Join-Path $package 'LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot 'NOTICE') -Destination (Join-Path $package 'NOTICE.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD_PARTY_NOTICES.txt') -Destination $package
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\third-party-sources') -Destination (Join-Path $package 'third-party-sources') -Recurse
$output = Join-Path $artifactRoot "vault-$version-setup.exe"
if (Test-Path -LiteralPath $output) { throw 'Installer already exists. Use a new artifact directory.' }
$toolRoot = Join-Path $env:LOCALAPPDATA 'tauri/nsis-3.13'
$compiler = Join-Path $toolRoot 'makensis.exe'
if (!(Test-Path -LiteralPath $compiler)) {
    $toolRoot = Join-Path $projectRoot 'artifacts/tools/nsis-3.13'
    $compiler = Join-Path $toolRoot 'makensis.exe'
    if (!(Test-Path -LiteralPath $compiler)) {
        $toolParent = Split-Path -Parent $toolRoot
        New-Item -ItemType Directory -Force -Path $toolParent | Out-Null
        $download = Join-Path $toolParent 'nsis-3.13.zip'
        Invoke-WebRequest 'https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip' -OutFile $download -UseBasicParsing
        $hash = Get-Sha256 $download
        if ($hash -ne 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8') { throw 'NSIS compiler checksum mismatch.' }
        Expand-Archive -LiteralPath $download -DestinationPath $toolParent
    }
}
& $compiler /INPUTCHARSET UTF8 "/DAPP_VERSION=$version" "/DAPP_REG_NAME=$RegistrationName" "/DPACKAGE_DIR=$package" "/DARTIFACT_DIR=$artifactRoot" (Join-Path $PSScriptRoot 'installer.nsi')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
$report = [ordered]@{ version=$version; bytes=(Get-Item -LiteralPath $output).Length; sha256=(Get-Sha256 $output); defaultDirectory='D:\vault'; dataPreservedOnUninstall=$true; silentUpdate=$true }
$report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $artifactRoot 'installer-size.json') -Encoding UTF8
$report | ConvertTo-Json
