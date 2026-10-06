# 开始使用

## 选择路径

| 你要做什么 | 从哪里开始 |
| --- | --- |
| 在一台主控上运行 opsd | [部署指南](deployment.md) |
| 修改 Rust、Agent 或 Web | [开发指南](development.md) |
| 理解组件、连接和数据边界 | [架构说明](architecture.md) |
| 管理已经运行的实例 | [运维手册](operations.md) |

## opsd 的工作方式

1. Hub 在主控上保存节点目录、认证信息、任务和审计记录。
2. Agent 在节点上主动建立 mTLS 连接，不需要 Hub 反向登录节点。
3. Hub 通过 Agent 下发结构化任务，Agent 只在本机执行允许的操作。
4. Web 控制台通过 Hub 的 HTTP API 查询状态和提交任务。
5. 需要公开展示的分享页使用单独的令牌和字段白名单，不直接暴露管理接口。

## 最小前置条件

### 运行 Hub

- Linux 主机或能够稳定运行 Docker 的主机。
- Docker Engine 和 Compose 插件。
- 一个用于访问控制台的域名或固定 IP。
- 可安全保存管理员密码、CA 指纹和安全入口的位置。
- 如果需要远程访问，反向代理必须保留完整的安全入口路径和正确的 Origin。

### 运行 Agent

- Linux 节点。
- 能够访问 Hub 的 Agent mTLS 端口，默认是 `8444`。
- 节点上安装 Docker Engine、Compose 插件以及所需的本机运维工具。
- 通过管理界面取得一次性注册令牌，并能安全保存令牌文件。

### 参与开发

- Rust 工具链，版本以 `rust-toolchain` 或 CI 配置为准。
- Node.js 与 npm。
- Linux/WSL 环境用于验证指标、主机盘点、数据库巡检和实验脚本。

## 下一步

- 新部署：阅读 [部署指南](deployment.md)。
- 首次本地开发：阅读 [开发指南](development.md)。
- 遇到连接、证书或入口问题：阅读 [故障排查](troubleshooting.md)。
