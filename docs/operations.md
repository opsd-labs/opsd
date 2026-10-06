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
- 管理地址和 SSH 端口；
- Agent 的 CA 校验结果；
- Docker、Compose 和本机工具的能力；
- 节点最近在线时间和最近一次指标采集时间。

节点离线时，页面应区分“没有新数据”和“指标为零”。历史结果可能继续显示，但必须结合采集时间判断。

## 指标和延迟

指标采集由 Agent 在节点本机完成。节点之间的对端延迟探测优先使用覆盖网地址，默认每 60 秒一轮；未测到、命令缺失或网络不通都表示“未知”，不等于 0 毫秒。

延迟矩阵包含节点间内部地址，不进入公开分享页。

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

存储流程分为主机盘点、磁盘预检、生成计划、执行计划和状态检查。执行前应确认设备路径、容量、挂载点、节点身份和计划有效期；不要把未核对的磁盘路径直接用于生产节点。

生产环境使用存储部署能力前，应先完成独立节点、数据和回滚验证。

## 终端和文件

终端和文件操作由 Agent 在节点本机执行。只向具备明确授权的管理员开放，限制会话生命周期和文件范围，并结合审计记录检查异常操作。

Hub 不挂载 Docker socket；需要访问 Docker 时由 Agent 使用节点本机权限执行。

## 分享页

分享页使用独立令牌和公开字段白名单。不要把内部地址、凭据、管理接口或数据库查询正文放入公开数据。撤销分享令牌后，旧链接应立即失效。

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

备份包含控制记录、任务以及 CA/身份私钥。Compose 文件、环境配置、镜像摘要和密码管理策略需要单独纳入受保护的部署备份。恢复不带回浏览器会话和旧注册令牌。

## 防火墙

默认构建只支持发现和计划，不执行生产防火墙写入。`experimental-firewall` 只能在隔离网络命名空间和专用实验环境中使用；WSL2 共用宿主机内核，不能替代独立 Linux 内核的生产验收。
