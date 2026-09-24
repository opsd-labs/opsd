# opsd

单主控与无头 Agent 的服务器管理系统。Rust 主控提供认证、任务与节点目录，Vue 3 提供管理界面，Agent 主动建立 mTLS 连接并在本机执行结构化任务。主控不挂载 Docker socket。

当前为开发版本，尚未满足完整首版验收。默认构建禁止防火墙写入；`experimental-firewall` 仅供隔离实验室构建。生产节点未接管，1Panel 和现有 Galera 部署未修改。详细验收缺口见 [实施状态](docs/implementation-status.md)。

## 本地开发与验证

```powershell
cargo test --all-targets
cargo build --bins
node tests/integration.mjs
cd web
npm ci
npx vitest run
npm run api
npm run build
npx playwright test
npm run dev
```

`npm run api` 是契约门禁：对 `docs/api/openapi.yaml` 做 OpenAPI 3.1 lint，并校验
`web/src/generated/api.ts` 与它一致（漂移检查）。改接口先改契约，再 `npm run api:generate`，
两个文件一起提交。契约的用途与约定见 [契约说明](docs/api/README.md)。

控制台部署在 `/{安全入口}/` 之下。开发服务器本身没有入口，需要把入口告诉 vite，
由代理为 `/api` 补上前缀（否则主控会按入口不匹配把请求丢弃）：

```powershell
$env:OPSD_ENTRANCE = "初始化时输出的安全入口"
$env:OPSD_TARGET   = "https://localhost:65535"
npm run dev
```

界面演示地址为 `http://localhost:5173/?demo=1`。十二节点数据明确标为演示，不能代表生产实时盘点。

高密度防火墙样板：`http://localhost:5173/?demo=1&page=firewall&node=C052`。统一组件、布局尺寸和验收结果见 [界面设计基线](docs/ui-density.md)。

数据库只读视图同样有演示数据（含未知、注意、异常三种状态）：
`http://localhost:5173/?demo=1&page=database`。截图可用
`node web/tests/shots.mjs database` 重新生成到 `docs/ui/`。

Linux 专有部分（指标采集、主机盘点、磁盘与数据库巡检）需要在 Linux 上验证：

```bash
wsl -d Debian -- bash tests/wsl-metrics-check.sh
```

本规划的分阶段蓝图见 [实施蓝图](docs/roadmap.md)。

## 主控部署

1. 将本项目放到主控，设置 `OPSD_ORIGIN=https://主控域名:65535`；远程访问还需设置 `OPSD_BIND=0.0.0.0`。
2. 执行 `docker compose build`。准备至少十二字符的密码文件 `admin-password`，权限设为 `0600`。
3. 初始化时指定实际使用的域名/IP，证书会绑定这些名称：

```bash
docker compose run --rm -v ./admin-password:/run/admin-password:ro hub init --password-file /run/admin-password --names hub.example.com
docker compose up -d
```

初始化会输出 CA 指纹和**控制台安全入口**，两者都要通过可信渠道保存。

### 安全入口

控制台默认监听 **65535**，并且**只响应带安全入口的地址**：

```
https://hub.example.com:65535/{安全入口}/
```

没有安全入口的请求（例如直接访问 `https://hub.example.com:65535/`）会被**直接丢弃**，
不返回任何 HTTP 响应，因此扫描者既进不来也拿不到「这里有一个控制台」的信号。

- 安全入口是首次 `hub init` 生成的 16 位随机串，字符集为大小写字母、数字与 `-` `_` `.` `~`。
- 可在控制台**设置 → 安全入口**中修改，立即生效；旧地址失效，当前会话有 60 秒宽限。
- **安全入口不是认证**，只是降低被扫描到的概率。会话 Cookie、CSRF 与来源校验仍然生效，
  未登录访问入口地址只会得到登录页。
- 忘记入口时无法从外部找回：请在初始化时保存，或停止主控后用 `opsd-hub` 读取控制库中的
  `settings/entrance` 记录。

健康检查走容器内**仅回环**的 65534 端口（明文，不经安全入口），由 `opsd-hub probe` 使用；
它不能从外部访问，因此也不是绕过入口的门路。

**Agent 注册是唯一的例外。** `POST /api/v1/enroll` 挂在入口前缀之外的根路径上并列入门禁白名单，
因为 Agent 在拿到客户端证书之前无法走 mTLS 通道。该端点凭据是一次性、十分钟有效、
随机的注册令牌，加上 Agent 侧的 CA 指纹核对，不依赖入口的隐蔽性。代价是这条路径会暴露自身存在
（返回 401 而不是直接断连），扫描者据此可以推断这里是一个 opsd 主控；白名单因此**逐条精确匹配**，
不会扩展成前缀，避免变成绕过门禁的通道。其余所有路径一律要求安全入口。

初始化输出 CA 指纹。通过可信渠道分发 `data/pki/ca.pem` 并核对指纹，不关闭 TLS 验证。浏览器需信任此 CA；反向代理必须保留正确 Origin 与**完整路径前缀**，不能在转发时重写掉安全入口那一层。8444 为端到端 mTLS 节点通道。初始化后移走密码文件。

默认使用 `data/control.db`。独立 MariaDB 可通过 `OPSD_DATABASE_URL=mysql://用户:密码@地址/数据库` 配置；需预先创建空库。该模式尚未通过真实 MariaDB 回归，不连接现有业务 Galera，也不支持在线跨库迁移。

## Agent 接入

在 Linux 构建 `cargo build --locked --release --bin opsd-agent`，将程序安装到 `/usr/local/bin/opsd-agent`。节点须有 Docker Engine、Compose 插件；防火墙发现依赖本机原有管理工具。

在管理界面添加节点，准确登记公网地址和实际 SSH 端口，取得十分钟一次性令牌。令牌写入权限 `0600` 的文件，然后运行：

```bash
opsd-agent --data-dir /var/lib/opsd-agent enroll \
  --hub https://hub.example.com:8043 \
  --agent-url wss://hub.example.com:8444/agent \
  --ca /root/opsd-ca.pem --fingerprint 核对后的CA指纹 \
  --token-file /root/opsd-token
install -m 0644 deploy/opsd-agent.service /etc/systemd/system/opsd-agent.service
systemctl daemon-reload
systemctl enable --now opsd-agent
```

Agent 以 root 执行宿主机运维，主控和节点数据目录均应限制访问。Agent 每小时检查证书，剩余三十天时通过 mTLS 续期并原子替换证书；端到端测试已验证强制续期后使用新证书接入。长期离线导致证书过期的恢复流程仍需补齐。

## 节点指标

Agent 每 15 秒采样一次主机指标并经既有 mTLS 通道主动上报，**不安装 node_exporter**：
CPU（总/每核/负载）、内存与交换、每个真实挂载点的容量与 inode、磁盘读写速率、
每张物理网卡的收发速率与错误、运行时长、进程数、TCP/UDP 连接数。

只读 `/proc` 与 `statvfs`，不执行外部命令。采样需要 Linux；其他平台不声明该能力位，
控制台据此显示「未上报指标」而不是 0。

历史按三层保存并自动清理：原始 15 秒 × 7 天、5 分钟 × 90 天、1 小时 × 1 年。
桶内标量取平均，挂载点与网卡明细取桶内最后一次。**没有数据的时段就是没有数据**——
曲线会断开，不会补零。

时钟偏移由主控在收到上报时估算并展示；该估计含单向网络时延，只用于识别明显偏差。

## 分享页

分享页是对外的只读状态面，位于 `https://主控:65535/share/{令牌}/`，
**不经过安全入口**，使用自己的令牌。在 设置 → 分享页 中开启并生成令牌，明文只在创建时显示一次。

隔离由三条机制保证：

- 控制台会话 Cookie 的 `Path` 限定在 `/{安全入口}/` 之下，浏览器**不会**把它发给分享路径；
- 分享面有独立路径与独立令牌，令牌撤销后该路径立即变成"连接被丢弃"；
- 公开响应只有字段白名单：显示名、地区、分组、标签、在线状态与主机指标。
  公网地址、EasyTier 地址、SSH 端口、容器与防火墙细节、任务与审计内容从不出现。

供 Grafana 等面板消费时使用 Prometheus 端点，需要**分享令牌 + API Key 双重校验**：

```bash
curl -H "Authorization: Bearer opsd_xxxxxxxx" \
  https://hub.example.com:65535/share/{分享令牌}/api/v1/public/metrics
```

令牌决定能读哪些节点，API Key 决定是否有权以机器方式读取，两者范围取交集。
指标标签只有 `node` 与 `display_name`，不含任何地址。

## 主题

设置 → 主题 中可以分别设置**控制台主题**与**分享页主题**，两者粒度不同：

- **控制台主题是令牌级**的：只覆盖颜色、圆角、密度等 CSS 变量，**不替换组件结构**。
  控制台里有终端、任务与表单，整体替换的风险不可接受。安装方式就是提交一份 `theme.json`：

```json
{
  "short": "indigo-soft",
  "name": { "zh-CN": "柔和靛蓝", "en": "Soft Indigo" },
  "version": "1.0.0",
  "surfaces": ["console"],
  "tokens": {
    "light": { "--accent": "#0F2540" },
    "dark": { "--accent": "#9BB5D6" }
  }
}
```

- **分享页主题是包级**的：会替换整个分享页前端。上传一个 zip，根目录需包含
  `theme.json` 与 `index.html`，其余资源随意。

```bash
curl -X POST -H "X-CSRF-Token: ..." -H "Content-Type: application/zip" \
  --data-binary @paper-theme.zip \
  https://hub.example.com:65535/{安全入口}/api/v1/themes/share
```

一个主题可以用 `surfaces: ["share", "console"]` 同时覆盖两个界面，但两个选择器分别设置。

安装时由主控**自行计算 SHA-256 摘要**，包内声明不作数；仅支持本地上传，
不接在线市场、不自动远程拉取。解压会拒绝越界路径与超大内容。
令牌值会写进 CSS 变量，因此含 `;`、`url(`、`@import` 等写法的值一律被拒绝。

**分享页主题的配置是公开可读的**，任何访客都能取到；不要把密钥放进主题配置。
主题页面与资源都带独立 CSP（`default-src 'none'`，默认禁止外联）。

## 堡垒机：终端与文件

节点页选中节点后可展开「堡垒机」，提供宿主机终端与文件管理。两者都走既有的 Agent 流通道，
**不开放任意宿主机命令接口**。

- **宿主机终端**：Agent 只允许白名单内的 shell（`/bin/bash`、`/bin/sh`），
  并清空自身环境后启动，因此终端里看不到节点身份与主控地址；起始目录同样受路径校验。
  会话建立与结束都写审计。
- **文件管理**：列目录、下载是只读操作，走流式请求-响应并记审计，**不进任务表**；
  上传、删除、重命名、改权限、建目录是变更操作，走任务机制，带幂等键并与防火墙、存储等变更串行。

文件路径在 **Agent 侧**强制校验，四条防线：

1. 只接受绝对路径，词法展开 `.` 与 `..`，并先解析已存在部分的符号链接再校验；
2. 打开文件带 `O_NOFOLLOW`，符号链接叶子一律拒绝；列目录、删除、重命名显式拒绝软链；
3. 硬禁 `/proc`、`/sys`、`/dev`、`/run/docker.sock`，以及 Agent 自己的数据目录与 PKI 目录；
4. 单次读取 1 MiB、单文件 64 MiB、递归删除 1 万条、列目录 5 千条上限。

上传先写暂存文件，摘要与大小核对通过才原子覆盖目标，**失败不会破坏原文件**。

> **权限提醒**：宿主机终端以 Agent 的权限运行（通常为 root）。首版只有本地管理员一个角色，
> 引入多用户之前必须先补上按节点的终端授权。

## 审计

设置 → 审计 中按时间、类别、结果查询。记录「谁在什么时候对哪个节点做了什么」，
保留一年。**审计不记录文件正文、命令全文、凭据或 SQL 正文**——审计要能被普通管理员查阅，
就不能成为新的泄漏面。

## 存储预检

存储页给出**结论式报告**：逐节点说明是否适合部署以及具体原因，随后是一致性检查与推荐拓扑。
右上角「全部节点盘点」会向所有在线节点下发一次只读盘点任务。

预检**只读**：不格式化、不挂载、不写任何磁盘。Agent 只带回事实，判断集中在主控完成，
因此结论可以被复核。

**只批准没有任何先前使用痕迹的磁盘。** 以下任一成立即排除：

- 承载 `/`、`/boot`、`/usr`、`/var`（分区风险会归到整盘上）；
- 已挂载且有文件系统；
- **有文件系统但未挂载**——很可能是数据盘，不挂载无法确认；
- 存在分区表；
- 属于 LVM、软 RAID 或加密卷；
- 是 USB 设备或容量为 0。

用过但已清理的盘请先 `wipefs` 再重新盘点。未采集到盘位的节点显示「待采集」，
**不代表不适合**——这两种情况的处理方式完全不同。

报告同时列出推荐拓扑（节点数 × 每节点盘数 × 最小盘容量）与扣除纠删码校验后的可用容量。
容量差异超过 25%、介质类型混用、链路低于 100 Mbps 都会在一致性检查中提示。

## 存储集群部署

部署分三步，每一步都留了可以停下来的位置：**保存定义 → 生成计划 → 执行计划**。
前两步不碰任何节点。

生成计划时会把「将要发生什么」固化下来：逐节点的设备路径、计划时的容量、挂载点，
外加一份**指纹**与 **5 分钟有效期**。执行前重新校验，任何一条不成立都拒绝：

- 设备集合变了（换盘、分区、容量变化）→ 指纹不匹配，计划作废；
- 计划过期；
- 某个节点离线或失去盘点结果；
- 计划含格式化步骤，却没有显式确认（界面上是勾选框，接口上是 `acknowledge_destructive`）。

执行是**逐节点串行、失败即阻断**：某台节点的磁盘准备失败时，后续节点一律不再执行。

节点侧在格式化之前**再核对一次现实**：设备存在、容量与计划一致、仍然没有任何使用痕迹、
挂载点仍在 `/data` 之下。判据与主控是同一套（`verify_device`），
因为计划与执行之间现实可能已经变了。

**S3 密钥不回传**：集群列表只说明密钥是否已设置，密钥本身只在服务端生成 Compose 时使用。

## 线路质量（对端延迟）

节点详情里的「线路质量」表显示本节点到各个对端的延迟，用于观察覆盖网（EasyTier）是否退化。

- 目标列表由主控下发，优先用**覆盖网地址**（节点之间真实的转发路径），带版本号，目录没变就不重发；
- 默认 **ICMP**（`ping -n -c 1`），不产生服务端日志也不需要额外特权；只有显式给出端口的对端才走 TCP；
- **每 60 秒一轮**，独立于 15 秒的采样循环，结果进同一套时序存储并带 `probed_at`；
- 从 ICMP 到 TCP 的每一种失败都带上原因。**没测到就是「未知」，不是 0**：
  0 毫秒是"同机"，与"网络不通"或"本机没有 ping 命令"是三件不同的事。

延迟矩阵带出节点之间的内部地址，因此**不进公开分享页**——分享页的字段白名单只放可以公开的事实。

## 数据库只读运维视图

数据库页回答的是「五节点 Galera 集群现在到底怎么样」，并且能回答「不知道」。

**只读**：不建库、不建用户、不改配置、不触发 SST、不 kill 查询。语句固定在节点侧的 Agent 里，
主控无法影响它们——因此没有 SQL 会经过网络。页面上也没有任何写入口。

**未知是一等状态。** 连不上、没权限、没配置都显示「未知」并给出原因，绝不退化成 0 或「正常」：
把采集失败显示成一片平静的绿色，是运维视图最危险的假象。节点离线时展示上次结果，
但会写明「以下是 N 秒前的巡检结果」。

页面分四块，各自独立——ProxySQL 连不上时 Galera 部分仍然有效：

- **Galera**：组件状态、成员数、本地状态、就绪/已连接、收发队列、流控暂停比例、
  认证冲突、集群与节点 UUID、`last_committed`、SST 捐赠者；
- **主机侧**：版本、连接与活跃线程、运行时长、`read_only`、binlog 位点；
- **ProxySQL**：后端（hostgroup、状态、权重、延迟）与最重的查询摘要；
- **备份链路**：hourly/daily/weekly 的最近成功时刻、体积、校验清单、归档头与定时器下次触发。

**判断在主控完成，阈值与告警规则逐条对齐**（成员数、队列 32、流控 0.01、writer 1、
备用 writer 4、小时备份 90 分钟、每日备份 26 小时…），所以这一页与告警不会互相矛盾。

**跨节点一致性检查**是这一页真正的价值：集群 UUID 不一致（已分区）、节点 UUID 重复
（数据目录被克隆）、各节点看到的成员数不一致、健康成员少于多数派。
只看了一部分成员时**不下多数派结论**——少巡检几台不等于它们不健康。

**凭据不出本机**：密码由 Agent 从运维既有的 0600 文件读取，只经子进程环境进入容器内的
客户端，既不上传主控也不进审计；权限不合格时拒绝读取。
Agent 的 `db-inspect.json` 里只有路径与账号名，因此不需要第二套秘密分发机制。

查询摘要**只返回摘要哈希与计数，不返回 SQL 正文**——正文里可能带着用户名、邮箱或令牌。
备份产物只检查归档头是不是 age 格式：解密私钥按设计不在节点上，因此不做解密校验，
界面上也不谎称做过。

### 节点侧配置

在承担数据库角色的节点上放置 `<Agent 数据目录>/db-inspect.json`：

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

配置里的路径指向运维**已有**的秘密文件，因此不需要新增秘密分发流程。
没有这个文件的节点会显示「未配置只读巡检账号」——那是未知，不是健康。

## 一致性备份与恢复

先停止主控，禁止同时运行旧主控与恢复后的主控。备份含控制记录、任务及 CA/身份私钥，部署 Compose 文件与环境配置另外保存到同一受保护备份位置。

```bash
docker compose stop hub
docker compose run --rm -v ./backups:/backups hub backup --destination /backups/备份名称
docker compose up -d
```

在另一环境准备全新空数据目录、相同部署配置，将备份挂载到 `/backups`：

```bash
docker compose run --rm -v ./backups:/backups:ro hub restore --source /backups/备份名称
docker compose up -d
```

保留相同 CA 和 Agent 可访问的域名及端口。恢复不带回旧浏览器会话和注册令牌。当前端到端测试验证了停止主控、备份、恢复到新目录、原 Agent 重连及历史任务保留；异机部署和磁盘损坏故障仍需补测。
