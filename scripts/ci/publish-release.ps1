# Publishes the signed installer and update manifest as the latest stable release.
# Every push to main publishes the version in package.json. An already published version is left unchanged.
param(
    [Parameter(Mandatory)][string]$Sha,
    [Parameter(Mandatory)][string]$Repository
)
$ErrorActionPreference = 'Stop'
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$tag = "v$version"
$notesFile = "docs/releases/$version.md"
if (!(Test-Path -LiteralPath $notesFile)) { throw "Release notes missing: $notesFile" }

$files = @("artifacts/vault-$version-setup.exe", 'artifacts/latest.json')
$existingText = gh release view $tag --repo $Repository --json isDraft,targetCommitish 2>$null
if ($LASTEXITCODE -eq 0) {
    $existing = $existingText | ConvertFrom-Json
    if (!$existing.isDraft) { Write-Output "$tag is already published; keeping immutable assets."; exit 0 }
    if ($existing.targetCommitish -ne $Sha) { throw "Draft $tag points to a different commit; refusing to overwrite." }
} else {
    gh release create $tag --repo $Repository --target $Sha --draft --title "vault $version" --notes-file $notesFile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
gh release upload $tag @files --clobber --repo $Repository
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
gh release edit $tag --draft=false --latest --repo $Repository
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
