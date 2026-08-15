import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import './styles/global.css'

// Linux 使用透明窗口实现圆角；Windows 保持不透明，避免 WebView2 白屏/透明框
if (navigator.userAgent.includes('Linux')) {
  document.documentElement.classList.add('linux')
}

const app = createApp(App)
app.use(createPinia())
app.mount('#app')