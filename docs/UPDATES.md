# 安装与在线更新

只发布 NSIS 安装包 `vault-<版本>-setup.exe`，不提供便携 ZIP 或独立 `vault.exe` 附件。设置页支持检查、下载、取消和重启更新；启动和每小时自动检查，由用户确认重启。

## 模块与权限

桌面 update_action → CoreSession.execute(Operation) → 核心权限服务 → UpdateService。五种更新操作均 DesktopOnly；MCP 工具发现和核心执行均不允许更新。UpdateService 管理版本、下载、验签和就绪状态；Windows 能力服务负责启动安装器。

桌面与 MCP 共享后台核心，在打开数据库前按程序绝对路径获取共享文件锁。多个 stdio 连接进程通过当前 Windows 用户专用命名管道与桌面共用一份 Rust 后端，不监听 TCP/HTTP 端口。

## 签名发布协议

稳定源：https://github.com/panda-student/vault/releases/latest/download/latest.json

签名信封格式：

```json
{"payload":"base64(UTF-8 JSON)","signature":"base64(Ed25519 signature over exact payload bytes)"}
```

签名载荷 schema=3 包含 version、platform=windows-x86_64、固定仓库/版本目录的 `vault-<version>-setup.exe` URL、安装包 sha256/size 和更新说明。先验签，只接受较新的正式版本。安装包限 64 MiB；下载完成后校验长度、MZ 文件头和 SHA-256，安装前再次校验。

公钥位于 `backend/updater/update-public-key.txt`；PKCS#8 DER 私钥的 Base64 文本保存为 GitHub Secret `VAULT_UPDATE_SIGNING_KEY`。不要随意更换已发行客户端信任的公钥，否则旧客户端无法验证新清单。更新验签与 Windows Authenticode/SmartScreen 签名互相独立。

## 更新流程

1. 下载完成并验签后，`prepare_install` 检查是否仍有其他桌面窗口或旧版 MCP 占用程序；有则拒绝，并提示先退出。
2. 桌面以 `setup.exe /S /D=<安装目录>` 启动安装器（`raw_arg`，`/D` 必须最后且不加引号），然后进入"installing"状态并退出。
3. 安装器内嵌的新版 `vault.exe` 作为引导程序执行 `--installer-replace`：暂停 MCP 请求并等待完成、关闭旧桌面、获取会话独占锁、旧 EXE 改名为 `previous.exe`、新 EXE 替换、按原数据目录启动并等待健康握手。
4. 健康握手失败时终止未确认的新版进程，恢复旧 EXE、重启原数据目录，并恢复连接。

安装器不打开密码库，只替换程序文件，数据目录保持原样。

## CI

`.github/workflows/ci-build.yml`：

- main 推送：测试、构建、生成安装包，发布 `preview-<commit>` 预发行附件；不提供稳定更新描述。
- `v*` 标签：校验版本，构建安装包，用签名密钥生成 `latest.json`，上传草稿 Release；附件齐全后发布为最新正式版。
- PR 和手动运行：验证并保留 7 天 Actions 附件，不发布正式版。
- 稳定版必须存在 `docs/releases/<版本>.md`，其内容作为清单的更新说明。

正式发布附件为 `vault-<version>-setup.exe`、`latest.json`、`SHA256SUMS`、`installer-size.json`。Secret 缺失或不匹配时发布失败，不允许无签名降级。

## 数据与失败处理

更新不删除、移动或上传 data。当前存储服务只接受 schema=4，运行时不迁移旧密码库。新版须在 60 秒内完成窗口与连接初始化并回报令牌，否则安装器终止新版进程并恢复旧程序。这不能证明全部业务功能正常。

安装器替换的是程序，不是数据库；新旧程序必须使用同一数据格式。缓存位于 `LocalAppData/LocalPasswordManager/updates`，不进入安装包。

## 安装包

- NSIS 3.13，使用现有工具链或校验固定 SHA-256 的编译器，不安装系统服务、不捆绑 WebView2。
- 按当前 Windows 用户安装，默认 `D:\vault`，无 D 盘时回退当前用户本地目录；目录页允许修改，重装沿用已有目录。
- 只写程序、授权文件、开始菜单快捷方式和当前用户卸载注册表。卸载删除安装文件，保留 data 和其他用户文件。
- 静默模式（`/S`）只用于在线更新，界面参数与手动安装相同。

构建暂存目录 `artifacts/vault-<版本>` 只供安装包内嵌，不发布。

## 版本过渡

0.6.1 及更早版本使用签名 ZIP 清单（schema=2），无法自动升级到本协议。首次升级需要手动运行新版 `vault-<版本>-setup.exe`，之后的版本可在线更新。升级前请保存界面修改并备份 data。

签名密钥初始化与恢复：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-updater.ps1 -UploadSecret`（只初始化/恢复本机密钥并上传 Secret，不打印私钥）。

## 验证

Rust 测试覆盖签名、版本、哈希、跨进程锁和清单字段。`scripts/preflight.mjs` 串联构建、安装包打包、安装覆盖、交接、回退和排空验收。

仅 debug 构建接受 `VAULT_UPDATE_TEST_SOURCE=http://127.0.0.1:<port>/latest.json`；仍使用正式公钥验签。发行构建不接受更新地址或公钥的环境覆盖。

`scripts/verify-update.mjs` 的安装包改为签名 schema=3 清单，但其中的启动失败回退用例仍需要一个真实的失败安装包作为夹具，尚未在 Windows 实机上重新运行。
