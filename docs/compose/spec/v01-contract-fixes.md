---
feature: v01-contract-fixes
status: delivered
updated: 2026-09-24
branch: feat/v01-contract-fixes
commits:
---

# opsd 首版第一批契约与实现修正

## Report

**What was built** — 指标缓存拆成 last_report / last_success，公开节点与总览只展示成功采样并显式给出 metrics_status；全部 22 个 JSON 写操作补齐 400 双 content + 415 + 422，api-contract.mjs 强制门禁；ClusterSpecInput / ThemeManifestInput（含嵌套）拒绝未知字段且旧数据可读；集成测试改用 ajv 标准 oneOf，并覆盖任务/主题/存储/数据库/指标代表响应。

**Verification** — cargo test --locked 190 PASS；npm run api PASS；npm test --prefix web 40 PASS；npm run build --prefix web PASS（>500KB chunk 警告）；node tests/integration.mjs PASS；node tests/negative-gates.mjs 6/6 按预期失败。cargo fmt --check 为 PRE-EXISTING（tests/entrance.rs 既有格式差异，未改无关文件）。

**Journey log** — 手改大 YAML 极易插坏 responses 层级，应 js-yaml 往返或按操作脚本补齐。Ajv 需把 $ref 改写成契约根绝对 URI 再编译，否则独立 $id 解析失败。overview 不得再透传 last_report 整包，否则与 additionalProperties: false 冲突。


## [S1] Problem

交叉审计与实施状态记录确认：契约已与源码对齐到可 lint / 可生成的程度，但仍有若干**已证实**的契约与实现偏差，以及验证深度不足——只靠 lint 和 TypeScript 生成发现不了「实现返回的正文与契约不符」，也发现不了「契约写了拒绝未知字段而实现会静默忽略」。首版生产承诺要求把这批问题收干净，再进入 WSL 实验室与防火墙矩阵。

具体缺口：

1. 分享机器接口契约曾标 `security: []`，与实现要求的 Bearer API Key 不符。
2. CSRF 曾是全局 security，与「只约束非 GET/HEAD」的实现不符。
3. WebSocket 流按协议只返回 101，契约中不可能出现的 403 需移除；且 Redocly 对「唯一 2xx 应为 101」缺专门检查。
4. `peer-addresses` 缺省时实现可能返回 null，契约要求 `version: 0, addresses: []`。
5. `authorize_key` 先写 `last_used` 再比较，条件恒假，写回永不发生。
6. 公开节点 `metrics_at` 取「最近一次上报」（含失败），界面语义是「最近一次成功采样」。**修订补充**：只改 `public_node` 过滤仍不够——内存缓存只保留「最近一次上报」，成功后紧跟失败会把成功值冲掉，必须拆成两个事实。
7. 框架级 400 / 415 / 422（axum `Json` 拒绝）正文是纯文本，契约未统一说明。**修订补充**：`FrameworkReject` 被引用但组件未定义，且多数 JSON 写操作未声明 415/422。
8. 首版变更请求 DTO 未统一 `deny_unknown_fields`。**修订补充**：`ClusterSpec`、主题 `Manifest` 仍忽略未知字段，且二者兼作持久化/包内清单，不能直接加严格规则。
9. 缺少对**真实 HTTP 响应**的 schema 校验。**修订补充**：自实现 `oneOf` 是「任一分支通过」，不是「恰好一个」。

## [S2] Design

### 契约

- 全局 `security` 仅为 `sessionCookie`；非 GET/HEAD 操作在操作级叠加 `csrfToken`。
- 分享机器接口（`/share/{token}/api/v1/public/...`）使用 `bearerApiKey` security scheme，禁止 `security: []`。
- `/api/v1/nodes/{id}/stream` 仅声明 101 / 400 / 401，**不声明 403 或任何 2xx**。
- `PeerAddressSet` 缺省语义：`{ version: 0, addresses: [] }`，类型非 null。
- `PublicNode`：
  - 指标字段与 `metrics_at` 来自**最近一次成功采样**；从未成功 → 全 `null`。
  - 新增 `metrics_status`：`ok` / `collect_failed` / `unknown`（从未上报）。采集失败时仍展示上次成功指标，并显式标失败。
- 框架级拒绝（`components.responses`）：
  - `FrameworkBadRequest`（400）：JSON 语法错误 → **text/plain**。
  - `UnsupportedMediaType`（415）：Content-Type 不对 → **text/plain**。
  - `UnprocessableEntity`（422）：字段缺失/类型错/未知字段 → **text/plain**。
  - 业务 400 仍是 `{"error"}`，与框架纯文本并列写在同一 400 的 `content`（json + text/plain）。
- **每个**含 `application/json` requestBody 的操作必须声明 400（双 content）、415、422。字节流上传（主题 zip）单独描述，不套 415 JSON 约定。
- 变更请求体与 `additionalProperties: false` 对齐：
  - 持久化模型（`ClusterSpec`、`Manifest`）保持宽松反序列化，保证旧控制库/旧主题包可读。
  - HTTP 入口用严格 `ClusterSpecInput` / `ThemeManifestInput`（含嵌套）`deny_unknown_fields`，再转换为内部模型。
- `api-contract.mjs`：枚举全部 JSON 写操作，检查 400 双 content、415、422、分享 Bearer、WS 101。
- 运行时 schema 校验用 `ajv`（+ `ajv-formats`），标准 `oneOf` 恰好一分支；封装状态码 / Content-Type / JSON schema / 纯文本框架错误。

### 实现

- 指标缓存 `HashMap<String, NodeMetrics>`：`last_report`（含 error）与 `last_success`（含 sample + received_at）。`ingest` 失败上报只更新 `last_report`；成功时两者都更新并写历史。
- `public_node` 读 `last_success` 出指标与 `metrics_at`，读 `last_report.error` 出 `metrics_status`。
- 历史仍只写成功采样，失败不写零值/伪时间点。
- `save_cluster` / 主题安装入口改收 Input DTO；`From<Input>` 映射；包内 `theme.json` 与控制库读继续用宽松 `Manifest`。
- 测试：成功→失败保留成功值并标失败；失败→成功恢复；连续失败不刷成功时间；重启无缓存 → null/unknown；Input 未知字段拒绝；旧数据可读；正常保存成功。

### 验证边界

- 单元：metrics 双事实、Input DTO、`last_used`、`metrics_status`。
- 契约：`npm run api` + 扩展后的 `api-contract.mjs`。
- 运行时：ajv 校验的 `tests/integration.mjs`；`cargo fmt --check`；`cargo test --locked`；`npm test --prefix web`；`npm run build --prefix web`。
- 负向：临时副本故意删 WS 101 / 匿名分享 / 删 422 / 响应多字段 / 未知请求字段 / 错误 Content-Type，门禁须失败。
- **不做**：WSL 矩阵、防火墙四后端、Docker 实验矩阵、生产验收。

## [S3] Out of Scope

- 拆 workspace crate、引入 Redis / MQ / 独立时序库。
- 指标、数据库巡检、存储集群、分享页、主题的生产化验收。
- 防火墙写入、`experimental-firewall` 打开。
- 前端导航收敛与实验标记。
- 真实多节点 / Galera / 生产 24 小时观察。
- WSL 实验室发行版（后续阶段）。

## Tasks

- [x] T1: 指标缓存拆成 last_report / last_success，PublicNode 增 metrics_status，并补成功↔失败序列测试 — acceptance: 4 个指标测试通过；公开字段在失败后仍保留成功值且 status=collect_failed (covers: S2)
- [x] T2: 定义 FrameworkBadRequest/415/422，全部 JSON 写操作补齐 400 双 content + 415 + 422，api-contract.mjs 强制检查 — acceptance: 脚本列出全部 JSON 写操作且通过；删任一 422 时脚本失败 (covers: S2)
- [x] T3: ClusterSpecInput / ThemeManifestInput（含嵌套）deny_unknown_fields，HTTP 入口转换；旧数据与主题包仍可读 — acceptance: 未知字段 422；旧记录/旧清单反序列化测试通过 (covers: S2)
- [x] T4: integration.mjs 改用 ajv 标准 schema 校验，覆盖登录/节点/peer/分享/任务/主题/存储/数据库代表响应 — acceptance: oneOf 恰好一分支；故意加 schema 外字段时校验失败 (covers: S2; depends: T2)
- [x] T5: 全量门禁（fmt/test/api/web test/build/integration）+ 六项负向验证 — acceptance: 全部 PASS 且报告实际数量；负向全部按预期失败 (covers: S2; depends: T1, T2, T3, T4)
