import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig(({ command }) => ({
  root: fileURLToPath(new URL('.', import.meta.url)),
  // Same reason as uni.tauri: the iframe page is served at
  // `unipkg://localhost/uni.tauri.hexview/` and its files live in `<package>/dist/`.
  base: command === 'build' ? 'uni.tauri.hexview/dist/' : '/',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  build: {
    outDir: fileURLToPath(new URL('./dist', import.meta.url)),
  },
  server: {
    port: 1421,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/target/**'],
    },
  },
}))
