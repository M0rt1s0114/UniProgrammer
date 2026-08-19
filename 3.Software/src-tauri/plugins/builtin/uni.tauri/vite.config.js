import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig(({ command }) => ({
  root: fileURLToPath(new URL('.', import.meta.url)),
  // The built page is served at `unipkg://localhost/uni.tauri/` while
  // its files live in `<package>/dist/`. The base therefore includes the
  // plugin path segment so the browser requests
  // `/uni.tauri/dist/assets/...`; the dev server keeps serving `/`.
  base: command === 'build' ? 'uni.tauri/dist/' : '/',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)), // 将 @ 映射到 src 目录
    },
  },
  clearScreen: false,
  build: {
    outDir: fileURLToPath(new URL('./dist', import.meta.url)),
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/target/**'],
    },
  },
}))
