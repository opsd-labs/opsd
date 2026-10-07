import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'node:path'

// 保留独立分享页入口，不修改内置控制台的源码或构建配置。
export default defineConfig({
  base: './',
  plugins: [vue()],
  build: {
    emptyOutDir: false,
    rollupOptions: {
      input: { share: resolve(__dirname, 'share.html') },
    },
  },
})
