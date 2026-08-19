import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
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
})
