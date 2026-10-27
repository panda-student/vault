# 后端架构规则

## 核心编排与模块自治

`core` 负责会话身份、统一操作入口、权限执行门和跨模块工作流。
功能模块通过 `contracts` 中的接口协作，具体实现由 `core/bootstrap` 注入。
模块不能反向依赖核心；不能为了测试增加反向 dev-dependency。
Tauri 和 MCP 只做传输、生命周期和协议适配，不打开仓储或复制业务策略。

完整依赖白名单在 [architecture.json](architecture.json)。它覆盖普通、开发、构建及平台条件依赖，新增 crate 或依赖必须同步说明职责并通过架构检查。
`contracts` 只存公共类型和边界接口；模块私有类型放在模块内部，不把所有实现细节集中成公共定义。
`common` 仅存真正共享的基础工具，不能成为业务代码的收纳目录。

## 模块拥有的资源

storage 拥有 SQLite schema、加密实现和持久连接；updater 拥有更新公钥及签名规则。
filling 拥有填写用例，platform 拥有 Windows UIA、输入、进程和运行协调实现。
credentials 拥有输入校验与修改合并，permissions 拥有策略判断与输出保护。
transfer 拥有导入导出容器、CSV 兼容规则、行校验和加密 KDF 参数。
资源路径随所属模块维护，不能依赖另一个模块的源码相对路径。

## 不可破坏的行为

- 桌面和 MCP 使用同一核心操作入口，但身份由启动组装绑定，不能从 RPC 参数指定。
- 每次执行检查权限；批量写入在事务内按最新设置复核，失败时整批回滚。
- MCP 不返回明文秘密，不提供永久删除或修改权限的操作。
- 导入导出容器不进入 MCP 输出；MCP 只能发起加密导出，口令、路径与明文不经过 MCP 线路。
- 窗口填写先验证单次目标，再延迟读取秘密；协议错误和日志不得携带秘密。
- storage 统一管理持久连接与事务，不由适配器临时打开数据库。
- 更新与覆盖安装保留等待在途请求、暂停、健康确认、回退及恢复连接的顺序。

## 测试归属

模块行为测试放在所属 crate 的 `src/tests.rs` 或相邻 `#[cfg(test)]` 模块。
跨模块事务、权限与工作流测试放在 `core/src/context/tests`；公开 RPC 测试放在 `core/tests`。
用接口替身验证模块边界，测试必须调用实际生产函数，不能在测试里重写同一算法。
Windows、Edge、安装和更新的真实验收仍放在 `scripts/`，单元测试通过不能替代这些证据。

桌面测试专用命令由显式 `test-support` feature 控制，默认关闭。
`npm run desktop:test` 生成 `artifacts/test-build/vault.exe`，不能发布。
正式构建使用 `npm run desktop:build`，并运行 `node scripts/verify-production.mjs` 检查隔离。

## 改动检查

1. `npm run verify:architecture`：依赖方向、模块归属和适配器边界。
2. `cargo fmt --all --check`、`cargo test --workspace --locked`。
3. `npm run build`、`npm run format:check`。
4. 按改动范围运行真实桌面、MCP、填写和更新验收，记录二进制哈希与限制。

规则参考 DBX 的模块自治、依赖白名单和测试归属思路，并按 vault 的权限、秘密处理和共享宿主要求制定。
