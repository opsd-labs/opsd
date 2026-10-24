# opsd

opsd 是一个用于管理多台 Linux 服务器的主控系统，主要由以下部分组成：

- **Hub**：运行在主控上的 Rust 服务，负责认证、节点目录、任务调度、审计和 Web 控制台。
- **Agent**：运行在被管理节点上的无头服务，主动通过 mTLS 连接 Hub，在本机执行结构化任务。
- **Web 控制台**：Vue 3 管理界面，提供节点、指标、任务和运维视图。
- **独立插件**：集成现有 EasyTier Web、Portainer 与 Silo，分别维护业务接口和页面。

Hub 不挂载 Docker socket。核心负责节点、双线路通信、任务和权限；ET、Docker／Portainer、Silo 由独立进程插件集成。插件页面独立分发，日常 Docker 业务统一通过 Portainer。

> **当前状态**：仍为开发／灰度版本。核心网络、独立插件和只读输出 API 已实现并通过本批云端验收，证据与支持限制集中记录在 [架构说明](docs/architecture.md#本批云端证据与边界)。本批未部署 C091、未格式化真实磁盘、未发布正式插件包；真实多节点 Silo 仍需单独部署验收。

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
| Docker／Portainer 插件 | 容器、Stack、镜像、网络、卷、日志与终端；多主机 Agent 接入待部署验收 |
| 终端和文件操作 | 已实现，使用前需按安全边界配置 |
| 数据库只读巡检 | 已实现解析和展示，真实集群回归未完成 |
| ET 与 Silo 插件 | 现有 ET Web 适配；明确选盘、部署恢复和桶用户集成，真实 Silo 集群未验收 |
| JSON 与 Prometheus 输出 | 独立只读 API Key、节点范围和字段白名单；分享页面已移除 |
| 审计、任务记录、备份与恢复 | 已实现并有自动化覆盖 |
| 防火墙发现与计划 | 默认可用 |
| 自动互信白名单 | 四后端隔离行为通过；现代 firewalld 独占表拒绝写入，通用防火墙管理仍为实验性 |

## 安全边界

- 控制台使用随机安全入口降低被扫描到的概率，但安全入口不是身份认证。
- 浏览器仍需要通过登录、Cookie、CSRF 和 Origin 校验。
- Agent 注册使用一次性、短时有效的令牌；注册后使用 mTLS 通道。
- CA 私钥、管理员密码、Agent 令牌和节点侧数据库凭据不得提交到仓库。
- 标准部署不需要 Hub 访问 Docker socket。
- 自动互信与通用防火墙管理分开；真实多节点接管、Silo 新盘部署和现有业务 Galera 在线迁移均需单独验收。

## 项目结构

```text
src/                 Rust Hub 与 Agent
web/                 唯一内置控制台（原 web3）
plugins/             SDK、ET、Docker／Portainer、Silo 独立包与页面
tools/               前端契约生成与检查工具
themes/              本机独立主题仓库，主仓库与镜像忽略
docs/api/openapi.yaml
                     HTTP 契约源文件
compose.yml          标准 Docker Compose 部署
Dockerfile           Hub 与 Web 镜像构建
tests/               集成、负向门禁和 Linux/WSL 实验脚本
deploy/              灰度实验脚本，不是标准部署入口
```
