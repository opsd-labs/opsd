# 控制台前端与独立主题

`web/` 是原 `web3`，也是唯一内置控制台。2026-10-07 已审查并修正维护者的正式 API 适配，通过云端构建与真实 Hub 浏览器验证；当前仍为灰度版本；2026-10-24 核心与插件迁移以 [架构说明](architecture.md) 为准，下方历史验收不代表新插件生产部署。

原 `web`、`web2` 分别迁入本机的 `themes/web/`、`themes/web2/`，对应公开仓库 [opsd-theme-web](https://github.com/opsd-labs/opsd-theme-web)、[opsd-theme-web2](https://github.com/opsd-labs/opsd-theme-web2)。主仓库忽略 `themes/`，Docker 上下文排除它，正式镜像不附带外部主题。

## 安装包与全局选择

主题包为 ZIP，根目录包含 `theme.json`、UTF-8 `index.html` 和构建资源。入口包含 `<head>`，资源地址使用相对路径。完整控制台清单示例：

```json
{
  "short": "web",
  "name": "经典控制台",
  "version": "0.1.0",
  "surfaces": ["console"],
  "console_frontend": { "api_version": 2 }
}
```

完整前端仅声明 `console`，不混用 `tokens` 或托管 `configuration`。没有 `console_frontend` 的清单继续按样式覆盖处理；分享专属入口与资源已退出。`default` 为保留标识，不可安装、覆盖或删除。

ZIP 上限为 20 MiB，解压总量上限为 64 MiB；复用已有路径与符号链接限制。完整控制台安装遇到已存在的标识返回 `409`，不会覆盖。安装与启用是两个步骤。

API 的唯一定义见 [OpenAPI](api/openapi.yaml)。以下路径均相对于 `/{安全入口}/api/v1`：

| 操作 | 接口 |
| --- | --- |
| 列表及类型 | `GET /themes`，`console_mode` 为 `frontend`、`tokens` 或 `null` |
| 当前选择 | `GET /themes/active`，新增 `console_frontend` 与 `console`，不再包含 `share` |
| 上传完整前端 | `POST /themes/console/package`，请求体为 ZIP 原始字节 |
| 解析发行版 | `POST /themes/console/repository/resolve`，提交 `{ "url": "公开 GitHub 链接" }` |
| 安装发行版资产 | `POST /themes/console/repository/install`，提交解析所得的 `url`、`tag`、`asset_id` |
| 启用完整前端 | `PUT /themes/active`，提交 `{ "surface": "console_frontend", "short": "web" }` |
| 卸载 | `DELETE /themes/{short}`，删除当前完整前端后回到 `default` |

仓库首页或 `/releases/latest` 解析最新稳定发行版；`/releases/tag/{标签}` 解析指定发行版。只列出已上传的 ZIP 资产。安装时重新核对资产所属发行版，再下载并复用上传安装流程。无公开发行版返回 `404`，无 ZIP 或资产不属于所选版本返回 `400`，GitHub 连接、限流或响应失败返回 `502`，超大资产返回 `413`。不克隆源码、不运行构建、不自动更新。

完整控制台在管理员会话的同源环境运行，应使用可信主题。当前完整前端必须声明接口版本 2，版本 1 的既有包不能启用；不兼容的当前选择回到内置前端，文件不被自动删除。

## 页面、资源与恢复

安全入口首页按 Hub 持久化的全局选择提供 HTML，登录前也生效。首页注入 `/{安全入口}/frontend/{short}/` 作为资源基址，HTML 使用 `Cache-Control: no-store`，资源使用 `no-cache`。切换选择不会删除旧主题资源，已打开页面仍可读取原资源；卸载会删除该主题资源。

固定恢复入口为 `/{安全入口}/frontend/default/`。访问它不会修改全局选择；登录后可将 `console_frontend` 设置为 `default`。外部主题未安装、入口文件缺失或无法读取时，首页回到内置前端。恢复入口可否完成界面管理仍取决于内置前端的业务适配，不能用 HTML 可访问替代功能验收。

主题文件存于 Hub 数据目录的 `themes/{short}/`，选择存于数据库。保留同一数据卷时，正常重启和更换镜像保留主题。现有备份命令没有打包主题资源；异机恢复数据库后需重新安装对应主题包，缺少资源时使用内置前端。

分享页面与独立分享构建已删除。公开展示由带只读 API Key 的 JSON／Prometheus 接口完成。Docker、网络、存储业务前端随内置控制台统一构建，0.2.0 后端模块包不再分发页面。

## 当前内置前端

Node 类型依赖与配置已补齐，原类型检查阻断已解决。前端使用 `web/src/api/generated.ts` 的正式类型，生成命令统一调用 `tools/api.mjs`，不另维护契约副本。

已接入的行为：

- 会话使用 `csrf`，节点使用 `{ connected, node }`，指标从 `{ nodes: [...] }` 读取；缺失内存或磁盘比例显示未知，加载失败显示错误。
- 构建资源使用相对地址，API 与 SSE 从安全入口首段生成路径；登出清理 SSE 重连与刷新定时器。开发代理支持入口环境变量和 WebSocket。
- 设置页支持完整前端列表、原始 ZIP 上传、GitHub 资产选择、安装、卸载及全局切换。切换会确认页面重载及未提交表单丢失，并保留查询参数；固定恢复入口可卸载当前主题返回内置前端。
- 输出设置创建与撤销独立只读 API Key，并限定节点范围；密钥只返回一次。入口轮换与审计沿用现有接口。
- Docker、网络、存储为固定一级页面，使用主站控件和深浅主题；包、依赖、版本、普通配置和秘密在对应页面管理。Docker 基础安装独立于 ET；网络页支持接入或部署 ET Web、客户端与首次组网；任务恢复和小屏导航见 [本批架构及验收](architecture.md#统一业务页面与模块初始化)。
- 节点页始终显示“添加节点”，现已提供一键安装命令、专属脚本和过期重建；当前流程及证据统一见 [Agent 一键安装](agent-installation.md)。刷新后只恢复注册摘要，Agent 完成注册才进入节点列表。
- 节点页展示两条控制连接、节点间两路探测、地址来源和白名单应用状态；数据库保留现有只读总览。

宿主机终端、文件、任务详情和通用防火墙界面仍需后续接入；Docker 模块的容器终端已通过实际 Portainer 验收。数据库和 Silo 的真实集群操作仍需专项验收，当前证据与限制集中见 [架构说明](architecture.md#统一业务页面与模块初始化)。两个独立主题未纳入本次业务页面适配。

## 独立主题开发

两个主题分别维护自己的业务代码，不共享运行时 SDK。`package.json` 的 `opsd.api_ref` 固定官方主仓库的契约提交；`npm run api:fetch` 下载该版本的唯一 OpenAPI 到忽略的 `.cache/`。不读取相邻主仓库，不手写第二份契约。

在任一主题仓库中：

```bash
npm ci
npm run api:generate
npm run package -- --out releases/theme.zip
```

打包命令先构建，再将 `dist/` 和 `theme.json` 放入 ZIP 根目录；不包含源码、依赖和 Git 数据。发行版链接安装必须先在独立仓库发布该 ZIP；只有源码标签或 GitHub 自动生成的源码包不构成可安装主题。未经集中验收不发布正式主题制品。

运行时演示入口、参数和操作分支已移除。原浏览器测试使用 `tests/fixtures/` 的 HTTP、SSE 与 WebSocket 夹具，不绕过运行时登录入口。夹具覆盖界面表现与请求形状，不能代替真实 Hub、Agent 的业务验收。

## 历史验收（2026-10-07）

验证渠道统一为现有云端 CI，不另建流水线或重复本地整批检查。受影响的 Rust 和 HTTP 测试覆盖类型兼容、安装、冲突、登录前分发、稳定资源地址、正常重启、入口丢失回退及卸载；GitHub 地址和发行版筛选使用确定性输入测试，不把公网不可达当作成功。

2026-10-07 本批已确认的云端结果如下。早期工作流按成功步骤复用证据，不能把当时的内置类型失败算作通过；后续内置适配已解除该阻断。

| 范围 | 结果与证据 |
| --- | --- |
| Rust 与完整主题声明 | [原有 Rust 测试通过](https://github.com/opsd-labs/opsd/actions/runs/37606799292)；严格输入修正后，仅重跑的 [主题单元测试通过](https://github.com/opsd-labs/opsd/actions/runs/37609086218) |
| 官方契约、经典单元测试与 ZIP | [契约检查及 40 项原有单元测试通过](https://github.com/opsd-labs/opsd/actions/runs/37606799292)；[经典主题打包通过](https://github.com/opsd-labs/opsd/actions/runs/37607376574)，[现代主题打包通过](https://github.com/opsd-labs/opsd/actions/runs/37608166256) |
| 经典浏览器测试 | [24 项通过](https://github.com/opsd-labs/opsd/actions/runs/37609548304)，使用接口夹具，未执行真实节点操作 |
| HTTP、负向门禁及静态页面 | [HTTP 集成、负向契约门禁、内置静态资源与独立分享页构建通过](https://github.com/opsd-labs/opsd/actions/runs/37609940773)；静态构建不能替代内置类型检查 |
| 分享页浏览器隔离 | [真实 Hub 与 Chromium 检查通过](https://github.com/opsd-labs/opsd/actions/runs/37610450010)，分享路径未携带控制台会话 Cookie，无效分享令牌连接被丢弃 |
| 内置前端审查修正 | [内置类型、控制台与分享构建、HTTP 集成通过](https://github.com/opsd-labs/opsd/actions/runs/37620199307)，该次浏览器脚本的空审计预期失败，后续修正 |
| 真实内置前端调用者 | [本轮 CI 通过](https://github.com/opsd-labs/opsd/actions/runs/37636707274)：真实登录、入口读取、审计空态、分享创建／撤销／开关、ZIP 上传与 409 冲突、非法仓库错误、全局切换、固定入口卸载恢复、真实 Agent 盘点任务及分享隔离；复用输入未变的构建和前期成功结果 |
| 内置节点注册入口 | [新增流程 CI 通过](https://github.com/opsd-labs/opsd/actions/runs/37644826410)：无节点添加入口、非法公网 IP 错误、宿主机与 Docker 注册信息、令牌文件下载、取消注册、刷新恢复待接入摘要、真实 Agent 注册上线及盘点；内置类型与页面重新构建，后端、契约和独立主题复用输入未变的成功结果 |

现有 CI 的 `reuse_passed` 只复用同批输入未变的成功步骤，`reuse_static_run` 复用已构建的页面制品。本轮 Docker 制品由现有发布流程构建并验收，未另跑 CI Docker 构建与本地检查。正式外部主题 ZIP 和三套前端全部业务的真实节点验收仍待完成。

后续仍需覆盖三套前端的终端、文件操作、有效 GitHub 发行版安装及无发行版／无 ZIP 的失败情况。内置前端当前具备本批验证范围，C091 按用户授权进行灰度更新；默认控制台仍不具备完整业务功能。

## C091 灰度部署（2026-10-07 历史记录）

当前一键安装版本的 C091 Hub 更新记录见 [Agent 一键安装](agent-installation.md)。

2026-10-07，节点注册入口补齐后，[现有镜像发布流程通过](https://github.com/opsd-labs/opsd/actions/runs/37645394182)，源码提交为 `882a9acb7dab98d6020a5f066059aadbb3cf15de`。开发分支只发布 `sha` 标签，未改动 `main` 与 `latest`。C091 的 `/opt/opsd-test` 沿用现有数据卷、CA、服务私钥、端口和安全入口，更新 Hub 镜像为：

```text
ghcr.io/opsd-labs/opsd@sha256:92e7fd0c022adf0672b3d184c8a4085db81e23f615c2c8dd1841fee47edcf3d6
```

Compose 与部署环境增加 `OPSD_AGENT_IMAGE`，使注册响应和 Docker 安装命令使用本次发布的固定 Agent 镜像；该配置没有启动或部署 Agent：

```text
ghcr.io/opsd-labs/opsd-agent@sha256:a002802886dfb311f2c6ed4faba05d4104d1a5c8c09aa94bc1e5a4085edf64d9
```

上一轮更新已用原 CA 和服务私钥补发覆盖当前访问地址的服务证书。本轮证书未变化，继续严格校验 CA 和访问地址。

部署后容器为 `healthy`，镜像版本标签与上述源码提交一致，固定 Agent 镜像配置已生效。管理员登录、节点列表、注册摘要和首页脚本均返回 `200`；当前脚本包含新增节点表单，安全入口保持不变。此前会话、主题、分享和资源 MIME 的成功证据继续复用。

C091 本次读取为 0 个已注册节点、1 条待接入记录，没有为部署验收创建注册令牌或节点。不能据此宣称远端 Agent 重连或历史任务恢复通过；真实 Agent 注册上线与盘点由上表的云端 Hub／Agent 浏览器验证证明。本次没有接管其他服务，也未部署 Agent。

## 请求次数与成本

本阶段正常、重试和回退均为 **0 次 LLM 调用**。上传与日常主题访问不依赖 GitHub。仓库解析通常发起 1 次 GitHub API 请求；安装通常为 1 次发行版核对加 1 次资产下载，下载可额外发生重定向，最多跟随 5 次。用户先解析再安装时通常共 3 次请求，后端不自动重试；单个网络请求超时为 60 秒。未增加发行版缓存，已安装资源从本机读取。GitHub 请求仅在用户主动解析或安装时产生，不引入模型延迟或费用。

内置适配也保持确定性处理，未增加模型请求、缓存或费用。客户端每轮并行读取节点、任务、指标和注册摘要共 4 项；SSE 事件沿用 250 毫秒合并刷新，断线后 5 秒重连，15 秒轮询作为回退。登出清理这些定时器，主题 HTML 与资源缓存规则沿用上文。收益由本批实际登录、上传、切换恢复和任务提交验证证明。

节点注册入口正常、重试和回退同样为 0 次 LLM 调用。生成注册信息新增 1 次 POST 和 1 次摘要 GET，取消新增 1 次 DELETE 和 1 次摘要 GET，不自动重试；相关 SSE 事件仍沿用上述合并刷新。令牌只留在当前页面内存和用户下载文件中，不写入浏览器持久存储。收益由令牌下载、刷新恢复及真实 Agent 上线验证证明。
