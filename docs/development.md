# 开发指南

## 环境

- Rust 工具链，版本以项目构建文件和 CI 为准。
- Node.js 与 npm。
- Linux 或 WSL 用于运行节点侧采集和实验脚本。
- Docker 用于镜像构建和容器 smoke test；普通 Rust/前端测试不要求宿主机 Docker。

## 常用验证

在仓库根目录运行：

```bash
cargo test --locked
cargo build --bins
node tests/integration.mjs
node tests/negative-gates.mjs
```

前端验证：

```bash
cd web
npm ci
npm run api
npm test -- --run
npm run build
```

Playwright 浏览器测试需要已安装浏览器：

```bash
npx playwright test
```

Linux 专有能力可在 WSL 中运行：

```bash
wsl -d Debian -- bash tests/wsl-metrics-check.sh
```

WSL2 共享宿主机内核，实验结果不能作为生产防火墙写入验收。

## 本地启动 Web

先启动一个可访问的 Hub，然后在 `web/` 目录设置入口和目标地址：

```powershell
$env:OPSD_ENTRANCE = "初始化时输出的安全入口"
$env:OPSD_TARGET = "https://localhost:65535"
npm run dev
```

开发服务器本身没有安全入口，Vite 代理会将 `/api` 请求补到入口前缀下。演示模式不会发起变更请求：

```text
http://localhost:5173/?demo=1
```

演示数据不代表生产实时状态。

## API 契约工作流

HTTP 契约源文件是 `docs/api/openapi.yaml`，前端类型生成物是 `web/src/generated/api.ts`。

修改接口时：

1. 先修改 `docs/api/openapi.yaml`。
2. 在 `web/` 运行 `npm run api:generate`。
3. 运行 `npm run api`，确认 lint、结构门禁和生成物检查全部通过。
4. 将契约和生成的 TypeScript 文件一起提交。

不要手工修改 `web/src/generated/api.ts`，也不要在其他 Markdown 中复制一份接口定义。

## 截图和视觉检查

视觉检查脚本不再默认写入仓库文档目录，必须显式指定输出路径：

```bash
node web/tests/shots.mjs database .data/screenshots/database
```

截图是本地检查产物，不应提交到仓库。

## 代码边界

- Hub 负责认证、权限、任务、聚合和持久化。
- Agent 负责节点本机事实采集和受控执行。
- Web 通过 API 与 Hub 通信，不直接访问节点或数据目录。
- 新功能应先明确能力边界、失败状态和审计要求，再实现跨层协议。
