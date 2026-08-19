import { createApp, h, onMounted, shallowRef, ref } from 'vue'
import HexViewer from './HexViewer.vue'
import { setLocale, type Locale } from './i18n'
import './styles.css'

// uni.tauri.hexview plugin contract: this package is an isolated web page
// loaded by the UI shell inside an iframe. The shell sends the buffer (and
// theme/locale) in, and this page sends edits / whole-buffer replacements /
// log lines back. See plugins/README.md for the complete message format.

interface HexMessage {
  type: string
  locale?: Locale
  theme?: 'dark' | 'light'
  baseAddr?: number
  data?: Uint8Array | ArrayBuffer | null
  offset?: number
  value?: number
  level?: string
  message?: string
}

const buffer = shallowRef<Uint8Array | null>(null)
const baseAddr = ref(0)

function toBytes(value: Uint8Array | ArrayBuffer | null | undefined): Uint8Array | null {
  if (value == null) return null
  if (value instanceof Uint8Array) return value
  return new Uint8Array(value)
}

function post(message: Record<string, unknown>) {
  window.parent.postMessage(message, '*')
}

function onEdit(offset: number, value: number) {
  post({ type: 'uniprog:hex:edit', offset, value })
}

function onReplace(data: Uint8Array) {
  buffer.value = data
  post({ type: 'uniprog:hex:replace', data })
}

function onLog(level: string, message: string) {
  post({ type: 'uniprog:hex:log', level, message })
}

window.addEventListener('message', (event) => {
  if (event.source !== window.parent) return
  const msg = event.data as HexMessage | null
  if (!msg || typeof msg.type !== 'string') return
  switch (msg.type) {
    case 'uniprog:hex:init':
      if (msg.locale) setLocale(msg.locale)
      if (msg.theme) document.documentElement.dataset.theme = msg.theme
      baseAddr.value = msg.baseAddr ?? 0
      buffer.value = toBytes(msg.data)
      break
    case 'uniprog:hex:update':
      baseAddr.value = msg.baseAddr ?? 0
      buffer.value = toBytes(msg.data)
      break
    case 'uniprog:hex:theme':
      if (msg.theme) document.documentElement.dataset.theme = msg.theme
      break
    case 'uniprog:hex:locale':
      if (msg.locale) setLocale(msg.locale)
      break
    default:
      break
  }
})

const app = createApp({
  setup() {
    onMounted(() => {
      post({ type: 'uniprog:hex:ready' })
    })
    return () =>
      h(HexViewer, {
        data: buffer.value,
        baseAddr: baseAddr.value,
        onEdit,
        onReplace,
        onLog,
      })
  },
})

app.mount('#app')
