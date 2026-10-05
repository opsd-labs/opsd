import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";

const entrance = process.env.OPSD_ENTRANCE || "";

export default defineConfig({
  base: "./",
  plugins: [vue()],
  build: {
    rollupOptions: {
      input: {
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
      "/share/": {
        target: process.env.OPSD_TARGET || "https://localhost:65535",
        secure: false,
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
