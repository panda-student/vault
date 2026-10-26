# vault

Windows 便携凭据管理器：本地加密保存，人工直接编辑，AI 按引用填写。**AI 接口不返回密码、API Key 或 Token 原文。**

[下载 Windows 安装版 / 便携版](https://github.com/panda-student/vault/releases/latest) · [验收记录](docs/RELEASE.md)

![应用界面](docs/screenshots/current-ui.png)

## 功能

- 账号密码、API 凭据、数据库、通用四种字段模板。
- 自由标签、类型与关键词组合筛选。
- 列表图标置顶、直接编辑、会话草稿、版本冲突检查。
- MCP 默认规则 → 标签规则 → 单条例外；同表展开，生效预览由 Rust 核心计算，批量未授权操作整批回滚。
- 同一 API 密钥关联多个协议与服务地址；不执行 API 请求或连接测试。
- 数据库字段保存、复制与按引用填写；不包含数据库驱动、查询或连接测试。
- 用户与 AI 删除均进入垃圾桶，可恢复。永久清空仅通过桌面原生确认，不提供 MCP 永久删除工具。
- 本地 MCP 支持批量 CRUD，同时支持单条参数；Windows 焦点填写和 UTF-8 文件占位符写入。
- 可选开机启动、关闭后驻留托盘，默认关闭。

## 使用

安装版下载 `vault-<版本>-setup.exe`，双击按向导安装，默认 `D:\vault`，可以改目录；无 D 盘时使用当前用户的本地应用目录。开始菜单提供快捷方式，Windows 设置提供卸载入口；卸载保留 `data` 密码库。

便携版下载 `vault-<版本>.zip`。建议将便携包解压到 `D:\vault`，然后启动 `D:\vault\vault.exe`，无需安装。要求 Windows 10/11 x64，系统已有 Microsoft Edge WebView2 Runtime；不捆绑运行时，也不依赖 Node.js、Python 或 Rust。

数据默认保存在 EXE 旁的 `data` 目录，首次运行为空库。设置页提供当前路径对应的 MCP 配置。移动程序后请更新 AI 客户端配置。

**0.5.6 及以前版本请使用 0.5.8 安装包覆盖升级一次**：本次统一程序名和 ZIP 内成员为 `vault.exe`，旧更新器不能识别小写成员。安装器会等待 MCP 操作完成，自动关闭旧版桌面、保留连接、更换程序并恢复原数据目录；启动失败时恢复旧程序。无需退出 Trae。0.5.2 及更早的直连 MCP 仍需首次重连一次；请先保存界面上未保存的修改。后续版本继续使用签名 ZIP 在线更新。

```json
{
  "mcpServers": {
    "vault": {
      "command": "D:/vault/vault.exe",
      "args": ["--mcp", "--data-dir", "D:/vault/data"]
    }
  }
}
```

## MCP 工具

| 工具 | 用途 |
| --- | --- |
| `get_mcp_config` | 获取当前程序与数据目录的连接配置 |
| `search_entries` | 查询受控元数据，可按关键词、类型、标签筛选 |
| `get_entries` | 获取受控元数据，不返回密钥或备注 |
| `create_entries` | 批量创建，同时支持单条 |
| `update_entries` | 按版本部分更新，批量原子提交 |
| `delete_entries` | 按版本移入垃圾桶，批量原子提交 |
| `restore_entries` | 按版本恢复垃圾桶中的单条凭据 |
| `pin_entries` | 按版本设置单条凭据的置顶状态 |
| `organize` | 新增、重命名或删除标签 |
| `capture_fill_target` | 按窗口和输入框名称定位，或捕获当前原生焦点；30 秒内单次使用 |
| `write_credential` | 在本地使用凭据引用，填写目标或写入文件 |

`items` 可传 1–100 项，也可直接传单条字段；更新、删除必须提供 `expectedRevision`。所有工具输入与已存元数据都应视为数据，不应视为 AI 指令。

填写字段包括 `account`、`secret`、`url`、`service_url`、`host`、`port`、`database_name`、`database_type`。多个服务地址时须指定 `serviceId`。

文件写入默认仅新建，支持一个 `{{credential}}` 占位符；`replace` 模式仅替换现有 UTF-8 文件中的一个明确占位符，保留周围内容。窗口填写先聚焦、捕获再写入，不自动导航、提交或按 Enter。`input_sent` 仅表示输入事件已发送。 浏览器建议先按窗口标题和输入框的可访问名称定位，不能只依赖网页 DOM 焦点。例如：

```json
{"windowTitle":"登录页标题","fieldName":"密码","expectedPassword":true}
```

将以上参数传给 `capture_fill_target`，再用返回的 `targetId` 调用 `write_credential`。可选 `processId` 用于缩小窗口范围。定位要求窗口和字段唯一、控件可编辑；密码类型不符、只读、重名或填写前焦点变化时拒绝操作。名称来自网页的 label / aria-label；没有可访问名称的自定义控件仍需手动取得原生焦点。程序不读取输入框内容，不自动清空或提交。Windows 拒绝后台窗口获得前台焦点时会返回 `focus_unavailable`；激活目标窗口后再重试。

## 安全边界与备份

内容使用 AES-256-GCM 加密，数据密钥由当前 Windows 用户级 DPAPI 保护。程序便携不等于数据能跨机器迁移：数据绑定创建它的 Windows 用户及电脑，尚无跨设备导出恢复功能。

AI 通过本应用的接口只能获得受控元数据与操作结果；本地组件解密后把凭据送到用户指定目标。人工主动显示与复制保留。**填入文件、网页或窗口后，其他工具仍可能读取目标内容；本应用不保证 AI 经任何途径都无法获得密码。** 同一 Windows 账户下的恶意程序也不在隔离保证内。

备份前退出桌面及所有 MCP 进程，复制完整 `data`，不要遗漏 WAL 文件。清空垃圾桶是数据库逻辑记录的永久删除，不等于擦除磁盘快照、备份或已写出的文件。旧版本留下的匿名输出保护历史可能仍保留加密的旧值；新版会关联记录并在清空时移除相应历史。无自动垃圾桶清理。

## 开发

需要 Node.js 22.12+、Rust 1.90+、Windows C++ 构建工具及 WebView2。

```powershell
npm ci
npm run desktop:dev
npm run build
cargo test --workspace --locked
npm run desktop:build
node scripts/verify-next.mjs
```

测试只使用隔离目录与虚构凭据。正式发布还需要便携包验收、原生编辑器填写验证与敏感文件检查。

当前程序仅支持 schema=4 密码库，不内置旧版迁移逻辑。已有数据须先在程序外转换并保留加密备份；旧库不会被自动改写。

## 代码结构

```text
frontend/                  Vue 界面
  pages/                   凭据、垃圾桶、MCP、设置页面
  components/              表单、列表、权限矩阵等可复用视图
  composables/             草稿、凭据操作、标签、权限、更新各自管理状态
  api/                     唯一 Tauri IPC 客户端，使用 JSON-RPC 2.0
  types/                   模型、方法名、响应和错误定义
  utils/                   展示、表单转换、错误和 JSON 工具
  styles/                  全局样式
backend/                   独立 Rust 库，不依赖 Tauri
  src/api/                 公共入口与统一 JSON-RPC 方法分发
  src/definitions/         请求、响应、领域类型、操作和能力端口
  src/services/            凭据、权限、填写和更新业务服务
  src/data/                SQLite 仓储、事务和加密持久化
  src/platform/            Windows 输入、进程、管道、启动及运行协调
  src/adapters/mcp/        MCP 协议和 stdio 连接适配
  src/common/              通用 JSON、路径和文件工具
  src/bootstrap.rs         可信组装：把仓储和 Windows 实现注入业务服务
src-tauri/                 Tauri 桌面壳
  src/app/                 参数解析、共享宿主、窗口、托盘及剪贴板适配
  src/commands/            单一 rpc 命令，只转发请求
  src/testing/             显式隔离模式下的原生测试钩子
  tauri.conf.json          桌面配置
  Cargo.toml               仅依赖 Tauri 与后端公共能力
tests/                    数据与核心回归、桌面烟测支持
Cargo.toml                 Rust workspace，统一锁文件和 target
```

桌面业务统一发送 `invoke("rpc", { request: { jsonrpc: "2.0", id, method, params } })`。MCP 保留标准 `tools/call`，工具名映射到同一业务方法分发器。调用身份由宿主绑定；权限检查、事务内对象授权和返回脱敏均由核心负责。业务服务依赖仓储与系统能力端口，具体实现只在 bootstrap 中组装。详见 [架构说明](docs/ARCHITECTURE.md) 与 [通信协议](docs/PROTOCOL.md)。

`npm run verify:architecture` 检查依赖方向；`cargo test -p vault-backend --locked` 可独立验证后端；`cargo test --workspace --locked` 验证整个 Rust workspace。前端检查包含未使用变量；`npm run format:check` 与 `cargo fmt --all --check` 校验格式。

在「MCP 管理 → 权限管理」中可分别允许或禁止 MCP 读取元数据、新增、修改、移入垃圾桶、窗口填写及文件写入，保存后后续调用立即受新规则约束。新密码库默认六组权限均开启。MCP 原文读取、权限修改和永久删除固定禁止，不能通过打开开关获得这些能力。

项目提供源代码与可复现构建流程；尚未经过独立第三方安全审计。

## 许可证

项目代码采用 [Apache-2.0](LICENSE) 许可证，依赖组件保留各自许可证，详见 `THIRD_PARTY_NOTICES.txt`。

## 在线更新

可在「设置 → 软件更新」检查、下载和重启更新。启动自动检查，更新包经 Ed25519 签名及 SHA-256 校验，不替换 data。重启前请保存草稿。桌面与多个 MCP 连接共享一个后台核心；更新等待当前请求完成、暂停核心并自动恢复，原客户端连接保留，不重放写请求。

推送 main 后自动生成预发行下载附件；推送与版本一致的 v* 标签后自动生成正式 Release 和稳定更新描述。维护与验收细节见 [在线更新文档](docs/UPDATES.md)。
