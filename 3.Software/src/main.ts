import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { useSettingsStore } from './stores/settings'
import './styles/global.css'

// Linux 使用透明窗口实现圆角；Windows 保持不透明，避免 WebView2 白屏/透明框
if (navigator.userAgent.includes('Linux')) {
  document.documentElement.classList.add('linux')
}

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)

// 挂载前加载 Setting.set、迁移旧 localStorage 并应用主题/语言，
// 避免组件首次渲染使用默认设置或出现主题闪烁。
const settings = useSettingsStore(pinia)
await settings.initialize()

app.mount('#app')
