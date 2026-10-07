# opsd

opsd 是一个用于管理多台 Linux 服务器的主控系统。它由三个部分组成：

- **Hub**：运行在主控上的 Rust 服务，负责认证、节点目录、任务调度、审计和 Web 控制台。
- **Agent**：运行在被管理节点上的无头服务，主动通过 mTLS 连接 Hub，在本机执行结构化任务。
- **Web 控制台**：Vue 3 管理界面，提供节点、指标、任务和运维视图。

Hub 不挂载 Docker socket。需要访问节点 Docker、主机或本地运维工具时，由节点侧 Agent 在本机执行并返回结构化结果。

> **当前状态**：opsd 仍处于开发和首版验收阶段。默认构建不会执行防火墙写入；`experimental-firewall` 仅用于隔离实验室。生产节点接管、真实数据库集群回归和独立 Linux 内核下的防火墙验收尚未完成，不能把当前版本视为完整生产就绪版本。内置控制台已迁为原 web3，其契约适配和占位业务仍待完成，详见 [配合说明](docs/console-themes.md)。

## 快速入口

| 目的 | 文档 |
| --- | --- |
| 部署一个 Hub | [部署 opsd](docs/deployment.md) |
| 了解项目并选择路径 | [开始使用](docs/getting-started.md) |
| 本地开发和运行测试 | [开发指南](docs/development.md) |
| 理解系统边界 | [架构说明](docs/architecture.md) |
| 日常运维 | [运维手册](docs/operations.md) |
| API 契约和类型生成 | [API 说明](docs/api.md) |
| 独立控制台主题与前端配合 | [控制台主题](docs/console-themes.md) |
| 安全模型 | [安全说明](docs/security.md) |
| 当前完成度 | [状态矩阵](docs/status.md) |
| 排查常见问题 | [故障排查](docs/troubleshooting.md) |

## 能力概览

| 能力 | 当前定位 |
| --- | --- |
| 节点注册、证书和 mTLS 通道 | 已实现，仍需生产规模验证 |
| 节点指标、主机盘点和历史曲线 | 已实现，Linux 环境需实际验证 |
| Docker 与 Compose 只读盘点 | 已实现 |
| 终端和文件操作 | 已实现，使用前需按安全边界配置 |
| 数据库只读巡检 | 已实现解析和展示，真实集群回归未完成 |
| 存储预检和集群管理 | 已实现流程，生产部署仍需验证 |
| 分享页和公开指标 | 已实现，字段由公开白名单控制 |
| 审计、任务记录、备份与恢复 | 已实现并有自动化覆盖 |
| 防火墙发现与计划 | 默认可用 |
| 防火墙写入 | 仅实验性构建，禁止视为生产能力 |

## 安全边界

- 控制台使用随机安全入口降低被扫描到的概率，但安全入口不是身份认证。
- 浏览器仍需要通过登录、Cookie、CSRF 和 Origin 校验。
- Agent 注册使用一次性、短时有效的令牌；注册后使用 mTLS 通道。
- CA 私钥、管理员密码、Agent 令牌和节点侧数据库凭据不得提交到仓库。
- 标准部署不需要 Hub 访问 Docker socket。
- 防火墙写入、真实多节点生产接管和现有业务 Galera 的在线迁移不属于当前默认支持范围。

## 项目结构

```text
src/                 Rust Hub 与 Agent
web/                 唯一内置控制台（原 web3）及独立分享入口
tools/               前端契约生成与检查工具
themes/              本机独立主题仓库，主仓库与镜像忽略
docs/api/openapi.yaml
                     HTTP 契约源文件
compose.yml          标准 Docker Compose 部署
Dockerfile           Hub 与 Web 镜像构建
tests/               集成、负向门禁和 Linux/WSL 实验脚本
deploy/              灰度实验脚本，不是标准部署入口
```
