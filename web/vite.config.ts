import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

const entrance = process.env.OPSD_ENTRANCE ?? ''
const target = process.env.OPSD_TARGET ?? 'https://localhost:65535'

export default defineConfig({
  // 相对路径，兼容 /{entrance}/frontend/default/ 等任意子路径部署
  base: './',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    host: '127.0.0.1',
    proxy: {
      [entrance ? `/${entrance}/api` : '/api']: {
        target,
        secure: false, // 允许自签名证书（mTLS 开发环境）
        changeOrigin: true,
        ws: true,
        headers: { Origin: target },
      },
    },
  },
  build: {
    target: 'es2022',
    rollupOptions: {
      output: {
        manualChunks: {
          xterm: ['@xterm/xterm', '@xterm/addon-fit'],
        },
      },
    },
  },
})
