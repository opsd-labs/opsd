# API 说明

## 契约源文件

`docs/api/openapi.yaml` 是核心 HTTP API 的唯一契约源文件，描述控制台、Agent 注册、通用插件入口与只读输出。业务接口分别由 `plugins/et/api.json`、`plugins/docker/api.json` 和 `plugins/silo/api.json` 维护；这些是插件自己的契约，不是核心契约副本。

前端类型文件 `web/src/api/generated.ts` 由契约生成，不应手工修改。

## 检查和生成

在仓库根目录：

```bash
npm ci --prefix tools
npm run api --prefix tools
```

`npm run api --prefix tools` 包含：

- OpenAPI 3.1 lint；
- 请求和响应结构门禁；
- 契约与生成 TypeScript 文件的漂移检查。

只重新生成前端类型：

```bash
npm run api:generate --prefix tools
```

## 修改接口的顺序

1. 修改 `docs/api/openapi.yaml`。
2. 更新必要的 Rust 路由、协议或前端调用。
3. 运行 `npm run api:generate --prefix tools`。
4. 在所选验证渠道运行契约、Rust 和集成检查；本轮使用现有云端 CI。
5. 将契约、实现和生成物一起提交。

不要维护第二份手写接口清单。需要查看完整路径、参数和 schema 时，直接阅读 OpenAPI 文件或使用兼容的 OpenAPI 工具渲染它。

## 行为约束

契约不仅用于生成类型，也用于约束实现行为。特别注意：

- WebSocket 升级端点必须保持契约定义的 `101` 响应；
- 根路径只读输出必须使用独立 Bearer API Key，不能继承管理会话或分享令牌；
- JSON 写操作需要声明框架拒绝、媒体类型错误和反序列化错误的响应；
- 契约中的未知字段、错误状态和内容类型约束必须与实现一致。

核心完整控制台接口版本为 2，插件进程接口版本为 1。迁移、历史任务兼容和本批证据见 [架构说明](architecture.md)。
