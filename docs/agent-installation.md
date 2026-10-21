# Agent 一键安装

## 使用方式

在内置控制台“节点 → 添加节点”选择宿主机或 Docker 模式，生成安装信息后，复制一键安装命令到目标机的 root Bash 终端执行。也可以下载专属 `opsd-install.sh`，将这一个文件放到目标机执行：

```bash
bash opsd-install.sh
```

命令与下载文件执行同一份脚本，自动准备令牌和公共 CA、取得程序、注册节点并配置常驻运行。首次注册凭据有效期为 10 分钟，关闭页面后无法重新取回；命令与脚本含凭据，不写入普通日志或公开记录。单行命令中的 Base64 仅用于传递脚本内容。

## 环境与行为

| 模式 | 要求 | 安装结果 |
| --- | --- | --- |
| 宿主机 | Linux x64／ARM64、root、Bash、systemd；Debian／Ubuntu 或 RHEL 系发行版 | 从官方 GitHub 固定发行版取得 musl 程序，安装至 `/usr/local/bin/opsd-agent`，启用 `opsd-agent.service` |
| Docker | Linux x64／ARM64、root、Bash、已有可用 Docker | 拉取配套固定摘要的双架构镜像，一次性容器完成注册后启动常驻容器，设置自动重启 |

缺少 `curl`、系统 CA、`tar`、`jq` 时使用 apt 或 dnf／yum 补齐，不执行系统升级或自动安装 Docker。目标机必须能访问 Hub、GitHub，Docker 模式还需访问 GHCR。Hub CA 单独交给 Agent 使用，不加入目标机全局信任库；下载、注册和状态查询均验证 TLS。

节点数据目录为 `/var/lib/opsd-agent`，权限 `0700`，凭据文件 `0600`。令牌只通过文件交给 Agent，注册成功后删除令牌与临时 CA；常驻容器不挂载令牌。安装器每两秒读取自身 mTLS 状态，最多三十次、总截止六十秒，只有 Hub 确认在线才报告完成。

## 失败与恢复

- 下载、注册失败直接停止，不自动重试；首次注册凭据过期时，从待接入记录重新生成安装信息。
- 重新生成会回填原参数，取消旧记录后创建新记录；取消失败先刷新状态，停止后续创建。
- 注册成功且身份完整时，重复执行原命令或脚本会复用该身份、补齐启动步骤，不生成第二个节点。已有身份对应其他节点或 Hub 时停止，不覆盖或迁移。
- 服务启动失败或 Hub 暂不可达时，保留身份和服务，输出 systemd／Docker 诊断命令；修复原因后可再次执行原脚本。
- 注册响应丢失或本地身份保存不完整时，需回控制台核实一次性令牌和节点状态，不承诺自动恢复这种情况。

旧令牌下载及手动接入保留在高级选项。开发构建未配置固定发行版本，或 Docker 未提供固定摘要时，创建响应的安装信息为 `null`，界面明确显示不可用原因。接口唯一契约见 [OpenAPI](api/openapi.yaml)。

## 云端验收

验证统一使用原有 CI 与发布工作流，未另跑本地检查、创建独立烟测框架或新工作流。业务制品来自 `f55c9f8594d8568d339b8450fc3b73c7d9a0bc72`；后续只修正验收脚本与发布流程，输入未变的程序、镜像及成功组合继续复用。复用允许验收脚本、发布工作流与无关 Markdown 文档改动，业务、依赖或 OpenAPI 契约变化时不能沿用该批制品。

| 范围 | 证据 |
| --- | --- |
| Rust、唯一契约、类型、构建、正式 HTTP 与浏览器 | [CI 通过](https://github.com/opsd-labs/opsd/actions/runs/39527205409)；浏览器覆盖命令复制、脚本下载、复制失败回退、过期摘要后的真实取消重建、取消失败阻止创建、摘要恢复及真实 Agent 上线。过期摘要使用界面夹具，实际到期拒绝由下项安装器验收覆盖 |
| 双架构发行包与配套 Hub | [发布构建步骤通过](https://github.com/opsd-labs/opsd/actions/runs/39527930443)；该次整轮因验收脚本的 Hub 重启等待失败，没有计为整轮通过 |
| ARM64 宿主机、ARM64 Docker、x64 Docker | 上述运行的[ARM64 宿主机](https://github.com/opsd-labs/opsd/actions/runs/39527930443/job/118990525939)、[ARM64 Docker](https://github.com/opsd-labs/opsd/actions/runs/39527930443/job/118990525890)、[x64 Docker](https://github.com/opsd-labs/opsd/actions/runs/39527930443/job/118990525907)分别通过实际安装、两个执行入口、重复执行、权限、令牌清理、服务重启恢复及在线确认 |
| x64 宿主机与失败恢复 | [定点验收通过](https://github.com/opsd-labs/opsd/actions/runs/39530435861/job/118994918594)，验收脚本版本 `259be8f4979ce103aa6d8ff87c391b0e9baa16ea`；同时覆盖发行包 404、令牌过期、CA 指纹不匹配、其他身份不被覆盖、注册成功后服务启动失败恢复，以及 Hub 不可达后恢复。失败用例核对实际原因，未把任意非零退出当作成功 |

四组合均使用 Ubuntu 24.04 原生 x64／ARM64 runner，真实从公开 GitHub 发行文件和 GHCR 镜像安装，没有用模拟架构替代。Debian 与 RHEL 系的发行版分支已实现，本次未宣称这些系统的宿主机 Agent 完整实测；C091 是 Debian 13，本批只更新其 Hub。

## C091 Hub 更新

2026-10-21，在上述验收通过后更新既有 `/opt/opsd-test` Hub，固定使用：

```text
Hub: ghcr.io/opsd-labs/opsd@sha256:14799f412b0d024be866b67884f4850f3d84b576b319df64fd84ca9c6b843184
Agent: ghcr.io/opsd-labs/opsd-agent@sha256:7be60dab85365956c3dcf48cfe9735d9850b38c9acbfda9c8509ab557c7ae868
Agent 发行版: agent-f55c9f8594d8568d339b8450fc3b73c7d9a0bc72
```

容器为 `healthy`，版本标签与制品源码一致，配套发行标签、内置镜像摘要和部署环境的 Agent 摘要均已生效。沿用原数据、CA、服务证书、端口和安全入口；严格校验原 CA 与访问地址后，登录、会话、节点列表和注册摘要均为 `200`，已分发含“复制一键安装命令”“下载安装脚本”的新页面资源。

部署验收没有在生产控制库创建令牌或节点，也没有安装目标机 Agent、接管其他服务。本次只发布固定提交的灰度制品，未更新 `main`／`latest`。请求次数、缓存和零模型调用约定统一记录在 [架构说明](architecture.md#首次安装)。
