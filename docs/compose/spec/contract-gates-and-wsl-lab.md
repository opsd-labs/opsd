---
feature: contract-gates-and-wsl-lab
status: delivered
updated: 2026-09-24
branch: feat/v01-contract-fixes
commits:
---

# opsd 契约门禁收口与 WSL 实验准备

## Report

**What was built** — 共享 HTTP 校验器严格匹配媒体类型并校验 JSON/纯文本 schema；OpenAPI 结构门禁检查 400/415/422 响应形状和全部内部引用；负向门禁在临时契约副本上运行，检查器异常不计为预期失败。WSL 脚本执行真实 Docker 与防火墙只读检查，控制面场景调用现有集成测试；缺少发行版或 Linux 产物时记录 `NOT_RUN` 并返回非零。

**Verification** — `cargo test --locked` 190 PASS；本轮 Rust 文件 Rust 2024 `rustfmt --check` PASS；`npm run api --prefix web` PASS（0 warning）；前端测试 40 PASS；构建 PASS；`node tests/integration.mjs` PASS；`node tests/negative-gates.mjs` 14/14 PASS。当前主机仅有普通 `Debian` WSL、没有 Docker CLI；Bootstrap 在修改系统前明确停止，矩阵生成 `NOT_RUN` 并以退出码 2 结束。没有创建发行版，也没有执行 WSL Docker/防火墙实验。

**Journey log** — 整文件覆盖 integration.mjs 会丢业务断言，应从 HEAD 恢复后增量打补丁。request() 必须回传 headers 才能断言 Content-Type。负向门禁里「服务端应返回 415/422」是正向断言，不能套 expectFail。

## 2026-09-25 复审修订

- HTTP 校验器以媒体类型主值精确比较，允许 `charset` 参数；纯文本可按响应 schema 校验；响应 schema 查询支持不同媒体类型，未解析的内部 `$ref` 会立即失败。
- OpenAPI 门禁验证 400 双媒体类型、415/422 的 `text/plain` 字符串 schema，并遍历检查悬空引用；`OPSD_CONTRACT_PATH` 可将门禁指向临时契约。
- 负向门禁只改写临时目录中的契约副本；要求检查器正常启动、非零退出且输出目标诊断。现有测试覆盖 14 个故障与拒绝场景。
- WSL 脚本验证 `opsd_` 发行版名并严格限定测试资源前缀；矩阵状态为 PASS/FAIL/SKIP/NOT_RUN，存在未运行项目时退出码非零。WSL 共用内核不代表生产防火墙验收。
- 当前机器缺 Docker CLI，且四个实验发行版不存在；WSL 创建与矩阵运行仍待满足前置条件后执行。
- OpenAPI 文件仍保留先前交付产生的大范围 YAML 风格重排；本轮未重写或重排该文件，以免改动已暂存的契约内容。


## [S1] Problem

上一轮已落地指标双事实、严格 Input DTO 与 JSON 写操作 400/415/422 声明，但运行时契约门禁仍不够可信：

1. `integration.mjs` 与 `negative-gates.mjs` 各自维护校验逻辑，覆盖面不一致；未统一走 `assertHttpResponse`（状态码 + Content-Type + schema / 纯文本）。
2. 负向门禁固定 `sleep 800ms`、硬编码端口、契约改写后无 `try/finally` 恢复，失败会留下脏契约或残留进程。
3. 未验证「删 400 的 text/plain」「删 JSON 响应 Content-Type」「破坏 $ref / JSON Pointer」会让门禁失败。
4. OpenAPI 仍有未使用的 `MetricsRecord`；部分对象未 `additionalProperties: false`；`api-contract.mjs` 不支持 JSON Pointer 的 `~0`/`~1` 转义。
5. 全仓库 `cargo fmt --check` 含既有差异，需要「只检查本轮修改文件」策略。
6. 尚无 WSL 实验室脚本，无法开始注册/任务/Docker/防火墙只读矩阵。

## [S2] Design

### 共享校验（tests/lib/http-assert.mjs）

- 导出 `loadContract`、`oasToAjvSchema`、`assertSchema`、`assertHttpResponse`、`responseSchema`、`deref`。
- `assertHttpResponse(res, { status, route, method, kind, schema })`：
  - `kind=json`：断言 JSON 体 + 可选 schema；
  - `kind=plain`：断言 `typeof data === 'string'` 且非空；
  - 可选断言 `content-type` 前缀（`application/json` / `text/plain`）。
- Ajv：`$ref` 改写为契约根绝对 URI；`type: [x,"null"]` / `oneOf`+null → `anyOf`+`{type:"null"}`；保留 `additionalProperties`、`items`、`enum`、`const`。

### 集成覆盖（tests/integration.mjs）

真实请求统一 `assertHttpResponse`，至少：

- 登录 200 / 401（JSON + schema）
- 非法 JSON → 400 `text/plain`
- 错误 Content-Type → 415 `text/plain`
- 未知字段 → 422 `text/plain`
- 节点列表、任务列表/详情、分享设置、主题安装、存储集群列表、数据库总览、指标总览、peer-addresses（JSON + schema）

### 负向门禁（tests/negative-gates.mjs）

- 主控启动改为健康检查轮询；端口可配置（`OPSD_TEST_*`）或动态占用。
- 临时目录、契约恢复、子进程终止一律 `try/finally`。
- 失败输出主控日志尾、实际状态码、清理结果。
- 保留六类检查，新增：
  1. 删除某 JSON 写操作 400 的 `text/plain` → `api-contract` 失败；
  2. 删除某 JSON 成功响应的 `Content-Type`（或 schema content）→ 契约/校验失败；
  3. 破坏 `$ref` / JSON Pointer（含 `~1` 路径）→ 门禁失败。
- 响应多字段负向：对**真实捕获响应的副本**注入额外字段，断言校验器拒绝；不声称服务器返回过该字段。

### OpenAPI 清理

- 删除未使用的 `MetricsRecord`；确认 `MetricsOverviewNode` / `PublicNode` / 任务·分享·主题·存储·数据库封闭对象均 `additionalProperties: false`。
- `api-contract.mjs`：`deref` 支持 JSON Pointer `~0`/`~1`；`npm run api` lint **0 warning 0 error**。

### Rust 格式

- 仅对本轮修改的 `.rs` 执行 `rustfmt --check`（`api.rs auth.rs channel.rs metrics.rs mod.rs share.rs storage.rs storage_cluster.rs theme.rs`）。
- 全仓库遗留差异记入 Report，不混入本轮。

### WSL 实验室（第二阶段脚本，不打开 experimental-firewall）

- `tests/lab/bootstrap-wsl.ps1`：创建/更新四个发行版 `opsd_hub_lab`、`opsd_c001_lab`、`opsd_c052_lab`、`opsd_c061_lab`（固定 Debian 工具链）。
- `tests/lab/configure-node.sh`：独立 Docker Engine、`opsd-test-` 资源前缀、节点身份参数。
- `tests/lab/run-wsl-matrix.ps1`：发行版层（注册/mTLS/心跳/续期/撤销/任务对账/备份恢复）→ Docker 矩阵 → 防火墙**只读**四后端发现 + netns 探测；输出实验报告（WSL 共享内核限制写明）。
- `tests/lab/cleanup-wsl.ps1`：失败也可回收容器/卷/网络/临时证书/（可选）发行版。

## [S3] Out of Scope

- 修改生产防火墙写入逻辑、打开 `experimental-firewall`。
- WSL 结果标记为生产防火墙就绪。
- 提交/合并 git。
- 新增运行时服务（Redis/MQ/时序库）。
- 前端功能改动。

## Tasks

- [x] T1: 抽出 `tests/lib/http-assert.mjs` 并改 integration/negative-gates 复用 — acceptance: 两文件均 import 共享模块；assertHttpResponse 覆盖状态码/Content-Type/JSON/纯文本 (covers: S2)
- [x] T2: integration 真实请求覆盖非法 JSON 400、错误 CT 415、未知字段 422 及全部代表成功响应 — acceptance: `node tests/integration.mjs` 通过且输出含上述用例 (covers: S2; depends: T1)
- [x] T3: negative-gates 健康轮询、动态端口、try/finally，并新增删 400 text/plain、删 JSON Content-Type、破坏 $ref/JSON Pointer 三类 — acceptance: 9/9 按预期失败；失败时无残留进程/契约 (covers: S2)
- [x] T4: 删除 MetricsRecord、封闭对象补 additionalProperties、api-contract 支持 ~0/~1、lint 0 warning — acceptance: `npm run api` 全绿无 warning (covers: S2)
- [x] T5: 本轮修改 Rust 文件 rustfmt --check + 全量验证清单 — acceptance: 修改文件 fmt 通过；报告列出全仓库遗留文件 (covers: S2; depends: T1, T2, T3, T4)
- [x] T6: 新增 tests/lab 四脚本与实验报告骨架 — acceptance: 脚本可独立阅读执行；cleanup 幂等；报告模板含 WSL 限制声明 (covers: S2)
