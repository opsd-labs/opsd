# 控制台前端与独立主题

`web/` 是原 `web3`，也是唯一内置控制台。迁移保留原源码和配置；它的业务适配由内置前端维护者完成。本轮提供后端、生成类型及配合说明，不能据此宣布默认控制台功能完整。

原 `web`、`web2` 分别迁入本机的 `themes/web/`、`themes/web2/`，对应公开仓库 [opsd-theme-web](https://github.com/opsd-labs/opsd-theme-web)、[opsd-theme-web2](https://github.com/opsd-labs/opsd-theme-web2)。主仓库忽略 `themes/`，Docker 上下文排除它，正式镜像不附带外部主题。

## 安装包与全局选择

主题包为 ZIP，根目录包含 `theme.json`、UTF-8 `index.html` 和构建资源。入口包含 `<head>`，资源地址使用相对路径。完整控制台清单示例：

```json
{
  "short": "web",
  "name": "经典控制台",
  "version": "0.1.0",
  "surfaces": ["console"],
  "console_frontend": { "api_version": 1 }
}
```

完整前端仅声明 `console`，不混用 `tokens` 或托管 `configuration`。没有 `console_frontend` 的旧清单继续按样式覆盖或分享页规则处理。`default` 为保留标识，不可安装、覆盖或删除。

ZIP 上限为 20 MiB，解压总量上限为 64 MiB；复用已有路径与符号链接限制。完整控制台安装遇到已存在的标识返回 `409`，不会覆盖。安装与启用是两个步骤。

API 的唯一定义见 [OpenAPI](api/openapi.yaml)。以下路径均相对于 `/{安全入口}/api/v1`：

| 操作 | 接口 |
| --- | --- |
| 列表及类型 | `GET /themes`，`console_mode` 为 `frontend`、`tokens` 或 `null` |
| 当前选择 | `GET /themes/active`，新增 `console_frontend`，保留 `console`、`share` |
| 上传完整前端 | `POST /themes/console/package`，请求体为 ZIP 原始字节 |
| 解析发行版 | `POST /themes/console/repository/resolve`，提交 `{ "url": "公开 GitHub 链接" }` |
| 安装发行版资产 | `POST /themes/console/repository/install`，提交解析所得的 `url`、`tag`、`asset_id` |
| 启用完整前端 | `PUT /themes/active`，提交 `{ "surface": "console_frontend", "short": "web" }` |
| 卸载 | `DELETE /themes/{short}`，删除当前完整前端后回到 `default` |

仓库首页或 `/releases/latest` 解析最新稳定发行版；`/releases/tag/{标签}` 解析指定发行版。只列出已上传的 ZIP 资产。安装时重新核对资产所属发行版，再下载并复用上传安装流程。无公开发行版返回 `404`，无 ZIP 或资产不属于所选版本返回 `400`，GitHub 连接、限流或响应失败返回 `502`，超大资产返回 `413`。不克隆源码、不运行构建、不自动更新。

完整控制台在管理员会话的同源环境运行，应使用可信主题。分享页仍使用独立入口、主题选择、Cookie 路径和 CSP；控制台主题包不能充当分享页主题。

## 页面、资源与恢复

安全入口首页按 Hub 持久化的全局选择提供 HTML，登录前也生效。首页注入 `/{安全入口}/frontend/{short}/` 作为资源基址，HTML 使用 `Cache-Control: no-store`，资源使用 `no-cache`。切换选择不会删除旧主题资源，已打开页面仍可读取原资源；卸载会删除该主题资源。

固定恢复入口为 `/{安全入口}/frontend/default/`。访问它不会修改全局选择；登录后可将 `console_frontend` 设置为 `default`。外部主题未安装、入口文件缺失或无法读取时，首页回到内置前端。恢复入口可否完成界面管理仍取决于内置前端的业务适配，不能用 HTML 可访问替代功能验收。

主题文件存于 Hub 数据目录的 `themes/{short}/`，选择存于数据库。保留同一数据卷时，正常重启和更换镜像保留主题。现有备份命令没有打包主题资源；异机恢复数据库后需重新安装对应主题包，缺少资源时使用内置前端。

内置分享页单独位于 `web/share/`，使用 `web/share.html` 和 `web/vite.share.config.ts` 构建。它沿用原分享页实现，不依赖内置控制台的组件与手写 API 类型。Docker 在控制台构建后补建分享入口，两者输出到 `web/dist/`。

## 内置前端维护者需要完成

当前云端类型检查阻断在 `web/vite.config.ts`：缺少 Node 类型声明，报 `path` 模块及 `__dirname` 不存在。先补齐前端的 Node 类型依赖与配置，再继续下面的契约和业务适配。本轮保留原文件，未代改这一部分。

1. 从 `web/src/api/generated.ts` 引入正式生成类型，替换手写类型。会话字段为 `csrf`，节点条目为 `{ connected, node }`，指标列表响应为 `{ nodes: [...] }`；注册令牌摘要和创建响应分别使用 `EnrollmentSummary`、`EnrollmentToken`。
2. 将构建资源基址设为相对路径。API、SSE 和 WebSocket 从安全入口首段生成地址，不从 `document.baseURI` 或 `/frontend/{short}/` 推导管理接口。开发代理补齐安全入口，并保留 WebSocket 支持。
3. 接入完整前端列表、上传、GitHub 解析与资产选择、安装、卸载及全局选择。完整前端、样式覆盖、明暗模式分别表达；明暗模式仍为本机偏好。
4. 切换完整前端前检查终端和未提交表单，必要时确认。保存全局选择后重新进入安全入口首页，可保留 `page`、`node` 查询参数；终端连接断开，后台任务继续执行。
5. 补齐 Docker、数据库、存储、设置等占位功能，按真实响应区分失败、无数据和执行中。API 类型生成及构建成功均不能证明这些业务已完成。

原 `web3` 的既有源码、配置和依赖文件本轮只迁移，未修改。新增生成文件、分享入口和构建配置为独立文件，供后续接入。

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

## 验收与请求成本

验证渠道统一为现有云端 CI，不另建流水线或重复本地整批检查。受影响的 Rust 和 HTTP 测试覆盖类型兼容、安装、冲突、登录前分发、稳定资源地址、正常重启、入口丢失回退及卸载；GitHub 地址和发行版筛选使用确定性输入测试，不把公网不可达当作成功。

2026-10-07 本批已确认的云端结果如下。引用的是各步骤结果，各次工作流并未全部通过；当前内置前端类型检查仍失败，不能算整批验收通过。

| 范围 | 结果与证据 |
| --- | --- |
| Rust 与完整主题声明 | [原有 Rust 测试通过](https://github.com/opsd-labs/opsd/actions/runs/37606799292)；严格输入修正后，仅重跑的 [主题单元测试通过](https://github.com/opsd-labs/opsd/actions/runs/37609086218) |
| 官方契约、经典单元测试与 ZIP | [契约检查及 40 项原有单元测试通过](https://github.com/opsd-labs/opsd/actions/runs/37606799292)；[经典主题打包通过](https://github.com/opsd-labs/opsd/actions/runs/37607376574)，[现代主题打包通过](https://github.com/opsd-labs/opsd/actions/runs/37608166256) |
| 经典浏览器测试 | [24 项通过](https://github.com/opsd-labs/opsd/actions/runs/37609548304)，使用接口夹具，未执行真实节点操作 |
| HTTP、负向门禁及静态页面 | [HTTP 集成、负向契约门禁、内置静态资源与独立分享页构建通过](https://github.com/opsd-labs/opsd/actions/runs/37609940773)；静态构建不能替代内置类型检查 |
| 分享页浏览器隔离 | [真实 Hub 与 Chromium 检查通过](https://github.com/opsd-labs/opsd/actions/runs/37610450010)，分享路径未携带控制台会话 Cookie，无效分享令牌连接被丢弃 |

现有 CI 的 `reuse_passed` 只复用同批输入未变的成功步骤，`reuse_static_run` 复用已构建的页面制品。已知的内置类型失败仍保留失败结果；不通过略过检查宣布整批成功。Docker 构建与发布门禁尚未执行，正式主题 ZIP、C091 部署及三套前端真实业务验收均待内置前端配合完成。

内置前端配合完成后再集中验收三套前端的真实登录、CSRF、任务提交、终端、文件操作、分享页隔离、仓库安装成功和无发行版／无 ZIP 的失败情况，然后通过现有发布门禁固定制品。完成这些步骤前不更新 C091，不宣称默认控制台功能完整。

本阶段正常、重试和回退均为 **0 次 LLM 调用**。上传与日常主题访问不依赖 GitHub。仓库解析通常发起 1 次 GitHub API 请求；安装通常为 1 次发行版核对加 1 次资产下载，下载可额外发生重定向，最多跟随 5 次。用户先解析再安装时通常共 3 次请求，后端不自动重试；单个网络请求超时为 60 秒。未增加发行版缓存，已安装资源从本机读取。GitHub 请求仅在用户主动解析或安装时产生，不引入模型延迟或费用。
