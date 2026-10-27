# 通信协议

桌面业务入口只有 Tauri `rpc` 命令，载荷为 JSON-RPC 2.0；业务模块不直接调用 Tauri。`frontend/api/desktop.ts` 生成递增 ID、检查响应 ID/版本，并把后端业务错误交给界面显示。类型定义在 `frontend/types/`，Rust 定义在 `backend/contracts/src/`。

```json
{"jsonrpc":"2.0","id":12,"method":"credentials.get","params":{"id":"credential-id"}}
```

成功响应为 `{"jsonrpc":"2.0","id":12,"result":{...}}`。失败响应为 `{"jsonrpc":"2.0","id":12,"error":{"code":-32000,"message":"...","data":{"code":"PERMISSION_DENIED","message":"..."}}}`。

| 错误码 | 含义 |
| --- | --- |
| -32700 | 原始 JSON 无法解析 |
| -32600 | 请求结构、版本或 ID 类型错误 |
| -32601 | 方法不存在 |
| -32602 | 参数错误，data.details 指明字段与原因 |
| -32000 | 业务失败，data.code 提供稳定的错误类别 |

有 ID（包括 null）产生响应；没有 ID 的合法通知执行但不响应。批量 JSON-RPC 请求按顺序执行，仅返回有响应的项，整个批量不承诺事务。空批量返回 Invalid Request。JSON-RPC 批量和业务 `params.items` 是不同机制：凭据 create/update/delete 的 items 批量在同一数据库事务中原子执行，最多 100 项，重复更新/删除 ID 被拒绝。

| 方法 | 参数 / 用途 |
| --- | --- |
| application.info | 当前版本、宿主 PID、目录、设置 |
| credentials.list / search / get | 列表、检索、按 ID 读取 |
| credentials.create / update / delete | 单条字段或 items；修改/删除必须提供 expectedRevision |
| credentials.pin / readSecret | 置顶、人工显示原文（后者仅桌面） |
| tags.list / organize | 标签目录、全局新增/重命名/删除 |
| trash.list / restore / empty | 垃圾桶；empty 由核心弹出原生确认，仅桌面 |
| settings.get / save / permissions.preview | 设置、权限配置及生效预览，仅桌面 |
| clipboard.copyField / copyText / browser.open / startup.set | 人工桌面能力 |
| updates.status / check / download / cancel / install | 在线更新，仅桌面 |
| mcp.configuration | 当前可用的 MCP 连接配置 |
| filling.capture / write | 一次性目标捕获、本地窗口或文件填写 |
| transfer.export / inspect / import / pending / respond | 桌面导入导出、文件预检与 MCP 导出确认；导入整批回滚 |
| transfer.mcpExport | MCP 请求加密导出；桌面在线输入口令，只返回状态与数量 |

MCP 使用 SDK 原生 JSON-RPC 协议。12 个工具映射到相同业务分发器，不额外包装一层自定义 MCP 协议。`Caller` 永不从 JSON 解析，MCP 无法调用权限修改、原文读取、永久删除或更新操作。接口授权后才进入业务；事务内再次核对最新策略和对象版本。秘密原文只在本地填写路径中解析，MCP 出口仍过滤协议帧。

`export_entries` 通过 `transfer.mcpExport` 进入核心：核心向桌面会话登记待确认请求，用户在 Vue 弹窗输入口令后由桌面写出加密 JSON 到数据目录的 `exports`。口令、路径与文件内容均不经过 MCP 线路；工具结果只有 `status/count/format/encrypted`。等待上限 120 秒。

`transfer.export`、`transfer.inspect`、`transfer.import`、`transfer.pending` 与 `transfer.respond` 仅桌面身份可调用。

Tauri 壳的 smoke_* / finish_smoke 是测试钩子，要求显式 --smoke-test 和隔离 --data-dir；不属于业务 JSON-RPC 或 MCP 工具。
