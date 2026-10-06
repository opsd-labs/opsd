# 部署指南

本文只描述 opsd 的标准 Docker Compose 部署。`deploy/` 下的灰度脚本属于内部实验工具，不是本文的替代路径。

## 1. 准备目录和配置

在主控上取得仓库或准备包含 `compose.yml` 的部署目录。远程访问时设置 Hub 对外使用的 Origin：

```bash
export OPSD_ORIGIN=https://hub.example.com:65535
export OPSD_CONSOLE_BIND=0.0.0.0
```

默认配置如下：

| 配置 | 默认值 | 说明 |
| --- | --- | --- |
| `OPSD_ORIGIN` | `https://localhost:65535` | 浏览器和 CSRF 校验使用的 Origin |
| `OPSD_CONSOLE_BIND` | `127.0.0.1` | Compose 在主机上绑定的控制台地址 |
| `OPSD_CONSOLE_PORT` | `65535` | 控制台端口 |
| `OPSD_AGENT_BIND` | `0.0.0.0` | Agent 通道在主机上的绑定地址 |
| `OPSD_AGENT_PORT` | `8444` | Agent mTLS 通道端口 |
| `OPSD_DATABASE_URL` | 未设置 | 未设置时使用 `data/control.db` |
| `OPSD_DATA_DIR` | `/var/lib/opsd` | 容器内数据目录 |

控制台默认只绑定主机回环地址。需要由反向代理或其他网络入口访问时，显式设置 `OPSD_CONSOLE_BIND`，并限制防火墙来源。

## 2. 构建镜像

使用仓库源码构建：

```bash
docker compose build
```

也可以使用 Compose 中默认的 GHCR 镜像。正式部署建议固定到已验证的镜像摘要，不要仅依赖可变的 `latest` 标签。

## 3. 初始化 Hub

创建只允许当前用户读取的管理员密码文件。密码至少使用 12 个字符：

```bash
umask 077
printf '%s\n' '替换为强密码' > admin-password
chmod 600 admin-password
```

使用实际访问 Hub 的域名或 IP 初始化证书：

```bash
docker compose run --rm \
  -v ./admin-password:/run/admin-password:ro \
  hub init \
  --password-file /run/admin-password \
  --names hub.example.com
```

初始化输出包含：

- CA 指纹；
- 控制台安全入口；
- 初始化结果和数据目录信息。

通过可信渠道保存 CA 指纹和安全入口。初始化完成后移走或删除密码文件。

## 4. 启动服务

```bash
docker compose up -d
docker compose ps
docker compose logs -f hub
```

控制台地址为：

```text
https://hub.example.com:65535/{安全入口}/
```

安全入口缺失或路径不完整时，请求会被门禁丢弃。Agent 注册是唯一不使用安全入口前缀的管理例外，具体见 [安全说明](security.md)。

## 5. 浏览器信任 CA

浏览器必须信任初始化生成的 CA。不要通过关闭 TLS 验证来绕过证书问题；应将正确的 CA 证书通过可信渠道分发，并核对指纹。

如果使用反向代理：

- 保留完整的 `/{安全入口}/` 路径；
- 保留正确的 `Origin` 和协议；
- 不要把 Agent 的 `8444` mTLS 通道代理成普通 HTTP；
- 不要把健康检查端口暴露到公网。

## 6. 接入 Agent

在 Linux 上构建 Agent：

```bash
cargo build --locked --release --bin opsd-agent
```

将 `opsd-agent` 安装到节点后，在控制台添加节点并生成一次性注册令牌。令牌只在短时间内有效，写入权限为 `0600` 的文件。准备 Hub CA 文件后运行：

```bash
opsd-agent --data-dir /var/lib/opsd-agent enroll \
  --hub https://hub.example.com \
  --agent-url wss://hub.example.com:8444/agent \
  --ca /etc/opsd/ca.pem \
  --fingerprint '<初始化时保存的 CA 指纹>' \
  --token-file /etc/opsd/enrollment-token
```

`--hub` 用于一次性注册请求，`--agent-url` 是注册完成后 Agent 连接的 mTLS 地址。注册命令会在 Agent 数据目录中写入身份材料；随后使用节点的 systemd 或其他进程管理器运行：

```bash
opsd-agent --data-dir /var/lib/opsd-agent run
```

注册完成后确认：

- 节点出现在控制台；
- Agent 使用 mTLS 通道在线；
- 节点能力和指标开始更新；
- 节点侧日志中没有持续的证书或连接错误。

### Docker Agent（仅 Docker 与只读采集）

控制台也可以生成 Docker Agent 命令。该模式使用宿主机 Docker socket，具备 Docker、Compose、日志、终端和防火墙只读发现能力，不开放防火墙写入、systemd 控制或存储破坏性操作。生产命令必须使用 CI 输出的固定镜像摘要：

```bash
# 令牌和 CA 只通过本地文件挂载，不放入环境变量或命令参数
docker run -d --name opsd-agent --restart unless-stopped \
  --network host --read-only --tmpfs /tmp:size=32m,mode=1777 \
  --cap-add NET_RAW --cap-add NET_ADMIN \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v /var/lib/opsd-agent:/var/lib/opsd-agent \
  -v "$PWD/opsd-ca.pem:/run/opsd/ca.pem:ro" \
  -v "$PWD/opsd-token:/run/opsd/token:ro" \
  -e OPSD_AGENT_MODE=container \
  ghcr.io/opsd-labs/opsd-agent@sha256:<固定摘要> bootstrap \
  --hub https://hub.example.com:65535 \
  --agent-url wss://hub.example.com:8444/agent \
  --ca /run/opsd/ca.pem --fingerprint '<CA 指纹>' \
  --token-file /run/opsd/token
```

没有固定摘要时只能用于实验，不能把 `main` 或 `latest` 当作生产版本。Agent 注册成功后确认节点在线，再手动删除宿主机令牌文件。
## 7. 数据库配置

默认使用本地控制库 `data/control.db`。如需使用独立 MariaDB，设置：

```bash
export OPSD_DATABASE_URL='mysql://用户:密码@地址/数据库'
```

数据库模式需要预先创建空库。当前版本尚未完成真实 MariaDB 回归，不连接现有业务 Galera，也不支持在线跨库迁移。

## 8. 升级和回滚

升级前：

1. 保存当前镜像摘要和 Compose 配置。
2. 停止 Hub 并完成备份，见 [运维手册](operations.md)。
3. 确认 CA、数据目录和 Agent 可访问的域名/端口不变。

升级后检查健康状态、控制台登录、Agent 重连和历史任务。回滚时使用之前保存的镜像摘要，不要重新拉取不确定的 `latest`。
