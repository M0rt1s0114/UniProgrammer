import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig(({ command }) => ({
  root: fileURLToPath(new URL('.', import.meta.url)),
  // The built page is served at `unipkg://uni.ui.webview/` while its files
  // live in `<package>/dist/`. A `dist/` base therefore makes the browser
  // request `/dist/assets/...`; the dev server keeps serving from `/`.
  base: command === 'build' ? 'dist/' : '/',
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
