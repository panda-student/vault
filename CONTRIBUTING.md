# 开发约定

先阅读 [实际架构](docs/ARCHITECTURE.md)、[后端规则](backend/ARCHITECTURE.md) 和 [通信协议](docs/PROTOCOL.md)。

新功能先明确所属模块和公共接口；跨模块流程放在 core，不让功能模块互相访问私有实现。
函数按单一职责划分，用名字表达操作，用注释解释安全约束、事务边界和非显然的原因。
公共接口定义放在 contracts，模块内部定义与实现一起维护。

测试跟随功能模块。真实 UI 测试检查点击后的状态和实际输入结果；编译成功、DOM 存在或静态截图不能代替交互验收。
测试使用临时目录和合成凭据，不接触用户已安装程序或真实密码库。
架构检查覆盖 dev/build 依赖，不允许通过测试依赖绕过模块边界。

本地执行规则文档中的检查。发布构建必须排除 test-support；不得提交密码库、签名私钥、令牌或本地验收产物。

## 验收

CI 在推送到 main 和 PR 上执行完整构建、打包与安装验收（`node scripts/preflight.mjs --ci`）。本地不再要求推送前验收，也不会阻止推送；需要手动复现完整流程时可运行 `node scripts/preflight.mjs`。
Rust 版本和 rustfmt 由根目录 `rust-toolchain.toml` 固定；换行规则由 `.gitattributes` 固定，Windows 新检出也必须通过格式检查。

验收报告在 `artifacts/preflight/latest.json`，每轮安装包和详细结果保存在对应独立子目录。

本地安装验收使用独立注册名、快捷方式及临时数据目录，避免覆盖已安装的 vault。它与正式安装包使用同一安装脚本、同一程序载荷，差别仅为注册命名空间；CI 的干净环境直接验证正式安装包。
