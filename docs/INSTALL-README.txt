vault · Windows x64

安装：运行 vault-<版本>-setup.exe，默认 D:\vault，可在向导中改目录。重装沿用原目录，卸载保留 data。
需要系统已有 Microsoft Edge WebView2 Runtime，不捆绑 Node.js、Python、Rust 或数据库驱动。

支持账号密码、API 凭据、数据库信息、自由标签、垃圾桶与本地 MCP 引用填写。MCP 服务和配置键均为 vault。AI 接口不返回密码、API Key 或 Token 原文；程序在本地解密并写入用户指定目标。目标文件或网页可能被其他工具读取。

在线更新：程序从 GitHub Release 下载签名安装包，校验后退出旧版、运行安装器替换程序并按原数据目录重启，同时恢复 MCP 连接。新版启动失败时自动恢复旧程序。覆盖安装前请保存界面修改。

数据默认在安装目录下的 data；自定义数据目录会保留。卸载只删除程序文件，不删除 data。内容使用 AES-256-GCM 加密，数据密钥受当前 Windows 用户和电脑的 DPAPI 保护，不能跨电脑解密。备份时退出程序并断开 MCP，复制完整 data，包含可能存在的 WAL 文件。

项目采用 Apache-2.0，第三方组件保留各自协议。请保留 LICENSE.txt、NOTICE.txt、THIRD_PARTY_NOTICES.txt 和 third-party-sources。
项目及下载：https://github.com/panda-student/vault
