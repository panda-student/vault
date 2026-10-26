# 功能模块拆分验证（2026-10-26）

本次将单一后端库拆为 core、contracts、credentials、permissions、filling、storage、platform、updater、mcp 和 common。测试下沉至所属 crate，跨模块工作流测试保留在 core。原有两项源码边界测试转移到架构检查脚本。

## 本地结果

| 检查 | 结果 |
| --- | --- |
| Cargo workspace 单元及 RPC 测试 | 72 项通过，无失败或忽略 |
| 架构规则自身测试 | 8 项通过，包括普通/dev/build 反向依赖和测试 feature 泄漏 |
| 实际依赖白名单及适配器检查 | 通过 |
| Rust 格式、前端严格类型检查与格式 | 通过 |
| 六套真实回归 | next、permissions、mcp-handoff、unified-backend、write、browser-fill 全部通过 |
| 更新替换协调 | 成功重启、失败回退、在途写入排空三种场景通过，两条原 MCP 连接恢复 |
| 正式构建 | 成功，6,313,984 字节 |
| 正式版隔离 | 拒绝测试参数，不创建密码库；正式 MCP 与真实桌面替换重启通过 |

六套回归检查了真实 WebView、桌面优先/MCP 优先时共享宿主、动态权限、记事本/编辑器输入和 Edge 密码框实际 DOM 值。所有凭据均为合成数据，使用临时目录，未替换用户安装或真实密码库。

测试版 SHA256：`25f286f5b9a7b44510607939beef639e34794b4d388cde81436f8af41c8e26b2`。
正式版 SHA256：`8a0d26ac13165f98d1e4915c270a61a0e1fdb5452ef66c63b29b7694eb56524c`。

本地证据在忽略目录中：`artifacts/regression/report.json`、`artifacts/modules-tests.log`、`artifacts/modules-handoff.json`、`artifacts/modules-rollback.json`、`artifacts/modules-drain.json`、`artifacts/modules-production-handoff.json`。

本轮更新验收调用真实替换协调器，未重新运行完整在线下载链路或生成 NSIS 安装包。签名、包校验及降级拒绝由 updater 模块测试覆盖。CI 文件已调整为先构建测试版验收、再构建不带测试能力的正式版；尚未推送，不能视为远端 CI 已通过。
