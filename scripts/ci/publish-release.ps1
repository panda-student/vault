# Publishes the installer (and signed manifest for stable tags) to a GitHub Release.
# Stable tags publish the latest release; other branches publish a prerelease per commit.
param(
    [Parameter(Mandatory)][string]$Ref,
    [Parameter(Mandatory)][string]$RefName,
    [Parameter(Mandatory)][string]$Sha,
    [Parameter(Mandatory)][string]$Repository
)
$ErrorActionPreference = 'Stop'
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$stable = $Ref.StartsWith('refs/tags/v')
$tag = if ($stable) { $RefName } else { 'preview-' + $Sha.Substring(0, 12) }
$files = @("artifacts/vault-$version-setup.exe", 'artifacts/installer-size.json')
if ($stable) { $files += 'artifacts/latest.json' }
$lines = $files | ForEach-Object { (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + (Split-Path -Leaf $_) }
$lines | Set-Content -Encoding utf8NoBOM artifacts/SHA256SUMS
$files += 'artifacts/SHA256SUMS'

$existingText = gh release view $tag --repo $Repository --json isDraft,targetCommitish 2>$null
if ($LASTEXITCODE -eq 0) {
    $existing = $existingText | ConvertFrom-Json
    if ($existing.targetCommitish -ne $Sha) { throw 'Release points to a different commit; refusing to overwrite.' }
    if (!$existing.isDraft) { Write-Output 'This commit is already published; keeping immutable release assets.'; exit 0 }
} else {
    $notes = @"
## 下载选择
- vault-$version-setup.exe：安装向导，默认 D:\vault，可修改目录；提供开始菜单和卸载入口，卸载保留密码库。

## 升级提示
应用内更新下载并运行本签名安装包，安装器自动等待 MCP 操作完成、关闭旧桌面并在安装后恢复原数据目录与连接，无需退出 AI 客户端。旧版本首次升级到安装包更新方式时需要手动运行一次本安装包。请先保存界面修改并备份 data。
Windows 10/11 x64，需要系统已有 WebView2 Runtime。密码库仍绑定原 Windows 用户和电脑。
"@
    $notes | Set-Content -Encoding utf8NoBOM artifacts/release-notes.md
    $title = "vault $version" + $(if (!$stable) { ' · Preview' } else { '' })
    gh release create $tag --repo $Repository --target $Sha --draft --title $title --notes-file artifacts/release-notes.md
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
gh release upload $tag @files --clobber --repo $Repository
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($stable) { gh release edit $tag --draft=false --latest --repo $Repository }
else { gh release edit $tag --draft=false --prerelease --latest=false --repo $Repository }
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

