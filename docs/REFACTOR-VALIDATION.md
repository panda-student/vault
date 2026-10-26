# 分层重构验收记录

本轮将 Vue、独立 Rust 后端与 Tauri 桌面壳拆分；JSON-RPC 定义和方法入口放在 api/definitions，服务不反向依赖 API、SQL 或 Windows 实现。详细模块关系见 [架构](ARCHITECTURE.md)，载荷规则见 [通信协议](PROTOCOL.md)。

## 已修复的问题

- 并行 RPC 会反复打开 SQLite 连接，实际复现 `SQLITE_PROTOCOL (15)`。数据层改为持有受互斥锁保护的连接，保留原有事务内授权、版本校验和原子批量操作。八个并发调用线程的回归测试同时覆盖读操作和 60 次写入，要求无错误且无记录丢失。
- 更新回退先确认旧程序窗口和监听健康，再恢复 MCP；恢复标记使用独立目录。安装回退测试还要求确实存在 vault.desktop 窗口，不能把仅有后台进程当成界面恢复。
- Windows 浏览器测试工具改用文件收集命令输出，避免后台进程继承输出管道后让已完成的命令持续等待。仅对执行命令前出现的工具配置冲突做一次重试。

## 自动化验证

| 检查 | 结果与证据 |
| --- | --- |
| Rust workspace 与独立后端 | 61 项原有测试 + 4 项公共 JSON-RPC 测试通过；日志 artifacts/rust-tests.log、artifacts/backend-tests.log |
| 前端 | 严格类型检查、未使用变量检查和 Vite 构建通过 |
| 模块依赖与格式 | verify:architecture、format:check、cargo fmt --all --check 通过 |
| MCP 与桌面 | CRUD、标签、垃圾桶原生确认、草稿、多环境、多账号、权限矩阵通过 |
| 权限与共享宿主 | 六组权限撤销/恢复、作用域规则、跨桌面和 MCP 可见性、暂停/恢复与不重放写操作通过 |
| 在线更新 | 隔离版本实际替换与失败回退，27 项检查通过；artifacts/update-acceptance/native-update-report.json |
| 安装协调器 | 旧程序自动退出、实际窗口恢复、MCP 保留连接、在途写入完成与只提交一次，通过；artifacts/refactor-installer-rollback.json、artifacts/refactor-installer-drain.json |
| 依赖声明 | 从 workspace 图重新生成，324 个组件，缺失声明 0 |
| 真实键盘填写 | 已通过：记事本、PowerShell ISE 保存文件的内容精确匹配；Edge 英文/中文密码框、普通输入框的 DOM 值匹配，事件为可信原生输入且未提交表单 |

完整套件于 2026-10-26 06:44（America/Los_Angeles）通过，六组测试均退出 0。日志与 EXE SHA-256 见 artifacts/regression/report.json。所有凭据均为隔离测试数据，真实安装目录和密码库未被修改。

最终回归程序 SHA-256：`248ce493fa0cbaf38db9a4a599ceb8d34130e0ee3b46fdc787239f5837ddb37d`。在线更新与安装协调器的功能修复构建另有哈希，记录在各自报告中；它们不与最终回归 EXE 混作同一构建证据。

原生输入最初因 Windows 锁屏阻断，解锁后补验。ISE 启动时曾将焦点移到模块组合框，测试定位现改为核对实际编辑器 RuntimeId，并等待焦点稳定；不会仅凭窗口位于前台就发送凭据。MCP 暂停测试等待宿主实际退出，避免将桥接确认与宿主退出当作同一时刻。

## 重跑验收

保持 Windows 解锁后执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-interactive-desktop.ps1
$env:VAULT_EXE = (Resolve-Path artifacts/refactor-review/vault.exe).Path
npm run verify:regression
```

该入口串行执行 next、permissions、mcp-handoff、unified-backend、write、browser-fill 六组套件，并实际验证记事本、PowerShell ISE、直接文件写入与 Edge 密码框。Windows 锁屏时前置检查会阻断真实输入测试，不会视为通过。本轮测试的 0.6.1 是隔离构建版本，不是发布标签，工程版本仍为 0.6.0。
