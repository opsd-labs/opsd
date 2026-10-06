# 三节点灰度部署

本目录的部署工具只面向 `c001`、`c091`、`c101`。默认执行只读预检；只有显式传入 `-Execute` 才会停止预检中识别为 active 的 1Panel systemd 单元。

## 边界

- C091 运行 Hub 容器和宿主机 Agent；C001、C101 只运行宿主机 Agent。
- Hub 使用 `/opt/opsd-test`，不复用 1Panel 项目、数据库或证书目录。
- Hub 容器不挂载 Docker socket；Agent 通过宿主机 systemd 访问本机 Docker 和防火墙只读接口。
- 首轮只做节点注册、Docker 只读盘点、任务对账和防火墙只读发现，不执行防火墙写入、默认策略修改、节点地址发布、`prune`、卷删除或业务批量更新。
- 失败时脚本只恢复它在本次运行中停止的 1Panel 单元，不删除任何业务容器、卷、网络或 1Panel 数据。

## 预检

在 Windows PowerShell 中准备一个仅当前用户可读的目录，存放三份一次性令牌和管理员密码。令牌文件名必须为 `c001.token`、`c091.token`、`c101.token`；这些文件不能放入 Git 仓库。

先只读检查：

```powershell
.\deploy\rollout-test.ps1 `
  -C001Host <C001-管理地址> -C091Host <C091-管理地址> -C101Host <C101-管理地址> `
  -C001KeyPath F:\Code\1server\.docs\ssh\C001-RackNerd-US-LADC3 `
  -C091KeyPath F:\Code\1server\.docs\ssh\C091-GreenCloud-JP-TYO `
  -C101KeyPath F:\Code\1server\.docs\ssh\C101-Lightlayer-CN-HK
```

脚本会记录系统版本、Docker、容器 ID、挂载、监听端口、磁盘空间和可识别的 1Panel 单元。任何端口冲突、旧 opsd 数据、已运行 Agent 或 SSH 错误都会停止流程，不会停止 1Panel。

## 执行灰度部署

部署前必须准备与 Linux `opsd-agent` ELF 二进制匹配的文件、管理员密码文件和三个一次性令牌：

```powershell
.\deploy\rollout-test.ps1 `
  -C001Host <C001-管理地址> -C091Host <C091-管理地址> -C101Host <C101-管理地址> `
  -C001KeyPath <C001-私钥> `
  -C091KeyPath <C091-私钥> `
  -C101KeyPath <C101-私钥> `
  -AgentBinary <Linux-opsd-agent-路径> `
  -AdminPasswordFile <密码文件> `
  -TokenDirectory <令牌目录> `
  -Execute
```

脚本将按顺序：预检三台节点、逐台保存并停止明确识别的 1Panel 服务、在 C091 拉取 `ghcr.io/opsd-labs/opsd:latest`、保存实际镜像摘要、初始化 Hub、安装三台 Agent 并启动 systemd。Hub 使用 `OPSD_CONSOLE_PORT` 和 `OPSD_AGENT_PORT`，避免与 C091 现有业务端口冲突。

`latest` 只用于选择发布通道；部署日志中的 `image-digest.txt` 才是本次实际运行版本。需要回滚时应使用该文件中的 digest，不能重新拉取不确定的 `latest`。

## 恢复

脚本异常时会自动恢复已经停止的 1Panel 单元。也可以在节点上按运行编号手工恢复：

```bash
bash deploy/remote-restore-onepanel.sh <run-id> <c001|c091|c101>
```

运行目录保存在本地 `.data/deploy/<run-id>`，该目录已被 Git 忽略。远端临时文件和令牌在 Agent 注册后会被清理；管理员密码只读挂载给一次性初始化命令。

## GitHub 前置条件

在推送代码前创建公开仓库 `opsd-labs/opsd`，并配置本地 `origin`。主分支合并后，`.github/workflows/publish-image.yml` 使用 `GITHUB_TOKEN` 推送公开 GHCR 镜像。不要把 SSH 私钥、密码、CA 私钥、令牌、节点清单或生产日志提交到仓库。
