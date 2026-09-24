import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";

// 控制台的静态资源与 API 都挂在 /{安全入口}/ 之下，因此构建必须使用相对路径，
// 否则部署到入口路径后资源会 404（而入口门禁会直接丢弃这类请求）。
// 分享页同样使用相对路径：它被服务在 /share/{令牌}/ 之下。
// 开发服务器本身没有入口，由下面的代理把 /api 补上入口前缀。
const entrance = process.env.OPSD_ENTRANCE || "";

export default defineConfig({
  base: "./",
  plugins: [vue()],
  build: {
    rollupOptions: {
      input: {
        // 控制台与分享页是两个独立入口，互不加载对方的代码。
        main: resolve(__dirname, "index.html"),
        share: resolve(__dirname, "share.html"),
      },
    },
  },
  server: {
    proxy: {
      "/api": {
        target: process.env.OPSD_TARGET || "https://localhost:65535",
        secure: false,
        ws: true,
        rewrite: (path) => (entrance ? `/${entrance}${path}` : path),
      },
      // 只代理带令牌的分享路径；否则会把本地的 share.html 入口也代理出去。
      "/share/": {
        target: process.env.OPSD_TARGET || "https://localhost:65535",
        secure: false,
      },
    },
  },
  // 单元测试只覆盖 src 下的纯函数；tests/ 由 Playwright 运行，两者不能互相收集。
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
