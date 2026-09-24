# HTTP 契约

`openapi.yaml` 是 opsd 主控 HTTP 接口的**契约源文件**。它同时服务三个目的：

1. 前端类型的唯一来源——`web/src/generated/api.ts` 由它生成，界面代码不再手写接口类型；
2. 接口变更的评审对象——先改契约，评审通过后再改实现；
3. 门禁的检查对象——lint 与漂移检查在 `npm run api` 里跑。

## 改一个接口的顺序

```powershell
# 1. 改契约
#    docs/api/openapi.yaml

# 2. 重新生成前端类型
cd web
npm run api:generate

# 3. 校验：lint 无 error，且生成物与契约一致
npm run api

# 4. 契约与生成物一起提交
```

只改实现不改契约，或改了契约不重新生成，都会让 `npm run api` 失败。**不要手改
`web/src/generated/api.ts`**——它是生成物，手改会在下一次生成时被覆盖。

## 覆盖范围

| 面 | 位置 | 认证 |
|----|------|------|
| 控制台 | `/{安全入口}/api/v1/...` | 会话 Cookie + CSRF 头（非 GET/HEAD）+ Origin |
| 一次性注册 | `/api/v1/enroll` | 一次性令牌，位于入口之外的白名单路径 |
| 分享面 | `/share/{token}/...` | 分享令牌（机器接口另需 Bearer API Key） |
| Agent mTLS | `/renew`、`/verify` | 客户端证书，独立监听面 |

控制台接口的 `/{安全入口}/` 前缀用 server 变量表达，不在每条路径里重复。

**刻意不收录**的端点写在 `openapi.yaml` 的 `info.description`「不在本文档范围内」一节：
Agent 常驻 WebSocket 通道 `GET /agent`（其内部协议与本文档分离）、回环健康检查
`GET /api/v1/health`、入口的 301 重定向。

## 两条最容易写错的约定

- **状态码不是统一的。** 只有节点操作 `POST /api/v1/nodes/{id}/actions` 返回 `202`；
  文件、盘点、巡检、存储等其他变更接口都是 `200`。以各操作自己的响应表为准。
- **错误正文有两种。** 业务错误是 `{"error":"<中文消息>"}`；门禁的 `403` 与请求体的框架级拒绝
  （400 / 422 / 415）是**纯文本**。

## 关于 `_extract/`

`_extract/*.json` 是 2026-09-24 从 Rust 源码逐模块提取的接口事实（含 `文件:行号` 证据），
用于编写本契约，**不是契约本身、也不参与构建**。保留它只是为了让评审能逐条回查某个字段
的依据。源码演进后它必然过时——有疑问时以 `openapi.yaml` 与源码为准。
