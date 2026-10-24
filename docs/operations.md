# 运维手册

## 日常检查

```bash
docker compose ps
docker compose logs --since=10m hub
docker compose exec hub opsd-hub probe
```

健康检查使用容器内回环监听，不经过安全入口，也不应对公网开放。控制台默认端口是 `65535`，Agent mTLS 通道默认端口是 `8444`。

## 节点管理

节点接入后，先确认以下信息准确：

- 节点标识和名称；
- 程序发现的地址来源、实际 ET 接口以及指定的 Hub 宿主节点；
- Agent 的 CA 校验结果；
- Agent 的插件、双线路与执行方式能力；
- 节点最近在线时间和最近一次指标采集时间。

节点离线时，页面应区分“没有新数据”和“指标为零”。历史结果可能继续显示，但必须结合采集时间判断。

## 指标和延迟

指标采集由 Agent 在节点本机完成。节点之间的公网与 ET 延迟分别探测，默认每 60 秒一轮；未测到、命令缺失或网络不通都表示“未知”，不等于 0 毫秒。

控制连接心跳、对端探测和 ET 直连／中继信息分开读取；内部地址和线路不进入只读公开输出。

## 数据库只读巡检

数据库页面只执行固定的只读巡检，不建库、不建用户、不改配置、不触发 SST，也不执行任意 SQL。连接失败、权限不足和未配置凭据都显示为“未知”，不会伪装成健康或零值。

如需启用数据库巡检，在节点 Agent 数据目录中放置 `db-inspect.json`。示例：

```json
{
  "container": "mariadb-galera",
  "galera": {
    "user": "mysqld_exporter",
    "port": 3306,
    "password_file": "/root/galera-secrets/cluster-secrets.env",
    "password_key": "EXPORTER_PASSWORD"
  },
  "proxysql": {
    "user": "admin",
    "port": 6032,
    "password_file": "/root/proxysql-secrets/admin.env",
    "password_key": "PROXYSQL_ADMIN_PASSWORD"
  },
  "backup_root": "/data1/server/db/backups",
  "expected_cluster_size": 5
}
```

密码只从节点侧已有的权限受限文件读取，不上传 Hub，也不进入审计记录。配置文件不存在或权限不合格时，页面显示未知原因。

## 存储管理

通过 Silo 插件盘点并逐节点明确选择新盘或已有路径。新盘确认清空的具体设备；已有路径不格式化。全部节点准备后才创建集群成员，失败后先核实实际状态再恢复。Portainer 只是编排入口，提交成功不能代替集群健康。

部署顺序、端口及真实磁盘／集群的未完成验收见 [架构说明](architecture.md)。

## 终端和文件

终端和文件操作由 Agent 在节点本机执行。只向具备明确授权的管理员开放，限制会话生命周期和文件范围，并结合审计记录检查异常操作。

Hub 不挂载 Docker socket。容器日志和终端经 Docker 插件桥接 Portainer；宿主机终端和文件保持原来的 Agent 路径。原生管理台操作由上游记录，不能视为 OPSD 的完整审计。

## 只读输出

在设置页创建只读 API Key，可限定节点范围；原文只返回一次。请求根路径的 /api/v1/public/nodes、/api/v1/public/summary 或 /api/v1/public/metrics，不附加安全入口，不再使用分享链接令牌。撤销 Key 后后续抓取失败。

```bash
curl --cacert /安全路径/ca.pem \
  -H "Authorization: Bearer $OPSD_EXPORT_KEY" \
  https://hub.example.com:65535/api/v1/public/nodes
```

密钥通过受保护方式设置到环境变量，不写入公开示例或日志。JSON 和 Prometheus 都仅返回公开字段。

## 备份与恢复

备份和恢复必须在 Hub 停止后进行，避免同时运行旧实例和恢复实例：

```bash
docker compose stop hub
docker compose run --rm \
  -v ./backups:/backups \
  hub backup --destination /backups/备份名称
docker compose up -d
```

恢复到全新数据目录时：

```bash
docker compose stop hub
docker compose run --rm \
  -v ./backups:/backups:ro \
  hub restore --source /backups/备份名称
docker compose up -d
```

备份包含控制记录、任务、CA／身份私钥，以及插件程序包、秘密和数据库中的部署进度；运行期 socket 不进入备份。Compose 文件、环境配置、镜像摘要和密码管理策略需要单独纳入受保护的部署备份。恢复不带回浏览器会话和旧注册令牌。

## 防火墙

自动互信按已认证节点目录收敛，支持范围与四后端隔离证据见 [架构说明](architecture.md#自动互信白名单)。通用策略页提供发现、计划和任务操作；Agent 的通用规则写入仍受 `experimental-firewall` 编译开关保护，直到完整策略应用与回滚隔离验收完成。WSL2 共用宿主机内核，不能替代独立 Linux 内核的生产验收。
