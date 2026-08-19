<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useProgStore } from '@/stores/prog'
import { locale } from '@/i18n'

// L1 UI contribution slot. `uni.hexview` is a required plugin, so the core
// refuses to boot when it is missing; this host only needs to load the plugin
// page and speak the postMessage contract documented in plugins/README.md.

const store = useProgStore()
const HEXVIEW_URL = 'unipkg://uni.hexview/'
const READY_TIMEOUT_MS = 8_000

const frameKey = ref(0)
const frameRef = ref<HTMLIFrameElement | null>(null)
const pluginReady = ref(false)
const loadFailed = ref(false)
let readyTimer: number | null = null
let suppressNextSync: Uint8Array | null = null
let themeObserver: MutationObserver | null = null

interface PluginMessage {
  type?: string
  offset?: number
  value?: number
  data?: Uint8Array | ArrayBuffer | null
  level?: string
  message?: string
}

function currentTheme(): 'dark' | 'light' {
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark'
}

function postToPlugin(message: Record<string, unknown>) {
  frameRef.value?.contentWindow?.postMessage(message, '*')
}

function sendState() {
  postToPlugin({
    type: 'uniprog:hex:init',
    locale: locale.value,
    theme: currentTheme(),
    baseAddr: 0,
    data: store.hexData ?? null,
  })
}

function armReadyTimeout() {
  if (readyTimer !== null) window.clearTimeout(readyTimer)
  readyTimer = window.setTimeout(() => {
    if (!pluginReady.value) loadFailed.value = true
  }, READY_TIMEOUT_MS)
}

function onFrameLoad() {
  armReadyTimeout()
}

function retry() {
  loadFailed.value = false
  pluginReady.value = false
  frameKey.value++
  armReadyTimeout()
}

function applyEdit(offset: number, value: number) {
  const current = store.hexData
  if (!current || offset < 0 || offset >= current.length) return
  // Mutate the existing buffer in place: hexData is a shallowRef and the
  // plugin view is the only renderer, so no full-buffer copy is needed here.
  current[offset] = value
}

function applyReplace(data: Uint8Array | ArrayBuffer | null | undefined) {
  if (!data) return
  const next = data instanceof Uint8Array ? data : new Uint8Array(data)
  suppressNextSync = next
  store.hexData = next
}

function onMessage(event: MessageEvent) {
  if (!frameRef.value || event.source !== frameRef.value.contentWindow) return
  const msg = event.data as PluginMessage | null
  if (!msg || typeof msg.type !== 'string') return
  switch (msg.type) {
    case 'uniprog:hex:ready':
      pluginReady.value = true
      loadFailed.value = false
      if (readyTimer !== null) {
        window.clearTimeout(readyTimer)
        readyTimer = null
      }
      sendState()
      break
    case 'uniprog:hex:edit':
      if (typeof msg.offset === 'number' && typeof msg.value === 'number') {
        applyEdit(msg.offset, msg.value)
      }
      break
    case 'uniprog:hex:replace':
      applyReplace(msg.data)
      break
    case 'uniprog:hex:log':
      store.addLog(
        typeof msg.message === 'string' ? msg.message : '',
        msg.level === 'success' || msg.level === 'warn' || msg.level === 'error'
          ? msg.level
          : 'info',
      )
      break
    default:
      break
  }
}

watch(
  () => store.hexData,
  (next) => {
    if (!pluginReady.value) return
    // A replace coming from the plugin was already applied locally and must
    // not be echoed back as a full-buffer copy.
    if (next === suppressNextSync) {
      suppressNextSync = null
      return
    }
    postToPlugin({
      type: 'uniprog:hex:update',
      baseAddr: 0,
      data: next ?? null,
    })
  },
)

watch(
  () => locale.value,
  (nextLocale) => {
    if (!pluginReady.value) return
    postToPlugin({ type: 'uniprog:hex:locale', locale: nextLocale })
  },
)

onMounted(() => {
  window.addEventListener('message', onMessage)
  themeObserver = new MutationObserver(() => {
    if (pluginReady.value) {
      postToPlugin({ type: 'uniprog:hex:theme', theme: currentTheme() })
    }
  })
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme'],
  })
  armReadyTimeout()
})

onUnmounted(() => {
  window.removeEventListener('message', onMessage)
  themeObserver?.disconnect()
  if (readyTimer !== null) window.clearTimeout(readyTimer)
})
</script>

<template>
  <div class="plugin-hex-host">
    <iframe
      :key="frameKey"
      ref="frameRef"
      class="plugin-frame"
      :src="HEXVIEW_URL"
      title="uni.hexview"
      @load="onFrameLoad"
    />
    <div v-if="loadFailed" class="plugin-error">
      <p>HexViewer 插件未能加载（uni.hexview）</p>
      <button class="btn btn-ghost btn-sm" @click="retry">重试</button>
    </div>
  </div>
</template>

<style scoped>
.plugin-hex-host {
  position: relative;
  width: 100%;
  height: 100%;
  background: var(--bg-base);
}

.plugin-frame {
  display: block;
  width: 100%;
  height: 100%;
  border: 0;
  background: var(--bg-base);
}

.plugin-error {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--color-danger);
  font-size: 12px;
}
</style>
