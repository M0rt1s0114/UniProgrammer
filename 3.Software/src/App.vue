<script setup lang="ts">
import { onMounted, ref, nextTick } from 'vue'
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
import { useProgStore } from '@/stores/prog'
import { t } from '@/i18n'
import OperationPanel from '@/components/OperationPanel.vue'
import ToolBar from '@/components/ToolBar.vue'
import HexViewer from '@/components/HexViewer.vue'
import LogConsole from '@/components/LogConsole.vue'
import StatusBar from '@/components/StatusBar.vue'

const store = useProgStore()
const appWindow = getCurrentWindow()

function minimizeWindow() {
  appWindow.minimize()
}
function maximizeWindow() {
  appWindow.toggleMaximize()
}
function closeWindow() {
  appWindow.close()
}

const logHeight = ref(180)
const isResizing = ref(false)

function onDividerMouseDown(e: MouseEvent) {
  isResizing.value = true
  const startY = e.clientY
  const startH = logHeight.value
  function onMouseMove(ev: MouseEvent) {
    const delta = startY - ev.clientY
    logHeight.value = Math.max(80, Math.min(startH + delta, window.innerHeight - 200))
  }
  function onMouseUp() {
    isResizing.value = false
    window.removeEventListener('mousemove', onMouseMove)
    window.removeEventListener('mouseup', onMouseUp)
  }
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

onMounted(() => {
  store.addLog('UnProg 已启动')
  fitWindowToSidebar()
})

// 启动时按左侧栏实际内容高度调整窗口，保证“文件/芯片/电压”等信息
// 一次完整显示，不用滚动。仅执行一次，之后尊重用户手动调整的尺寸。
async function fitWindowToSidebar() {
  await nextTick()
  await new Promise((resolve) => setTimeout(resolve, 150))
  try {
    const sidebar = document.querySelector<HTMLElement>('.sidebar')
    if (!sidebar) return
    const titleH = document.querySelector<HTMLElement>('.titlebar')?.offsetHeight ?? 40
    const toolbarH = document.querySelector<HTMLElement>('.toolbar')?.offsetHeight ?? 58
    const statusH = document.querySelector<HTMLElement>('.status-bar')?.offsetHeight ?? 28
    const chromeH = titleH + toolbarH + statusH
    const availH = window.screen.availHeight
    const maxH = Math.max(680, availH - 48)
    const targetH = Math.min(Math.max(680, Math.ceil(sidebar.scrollHeight + chromeH)), maxH)
    await appWindow.setSize(new LogicalSize(window.innerWidth, targetH))
  } catch (err) {
    console.warn('fit window to sidebar failed:', err)
  }
}
</script>

<template>
  <div class="app-shell" :class="{ 'is-resizing': isResizing }">
    <header class="titlebar" data-tauri-drag-region>
      <div class="titlebar-icon">
        <svg width="16" height="16" viewBox="0 0 640 640" fill="currentColor">
          <path
            d="M240 88C240 74.7 229.3 64 216 64C202.7 64 192 74.7 192 88L192 128C156.7 128 128 156.7 128 192L88 192C74.7 192 64 202.7 64 216C64 229.3 74.7 240 88 240L128 240L128 296L88 296C74.7 296 64 306.7 64 320C64 333.3 74.7 344 88 344L128 344L128 400L88 400C74.7 400 64 410.7 64 424C64 437.3 74.7 448 88 448L128 448C128 483.3 156.7 512 192 512L192 552C192 565.3 202.7 576 216 576C229.3 576 240 565.3 240 552L240 512L296 512L296 552C296 565.3 306.7 576 320 576C333.3 576 344 565.3 344 552L344 512L400 512L400 552C400 565.3 410.7 576 424 576C437.3 576 448 565.3 448 552L448 512C483.3 512 512 483.3 512 448L552 448C565.3 448 576 437.3 576 424C576 410.7 565.3 400 552 400L512 400L512 344L552 344C565.3 344 576 333.3 576 320C576 306.7 565.3 296 552 296L512 296L512 240L552 240C565.3 240 576 229.3 576 216C576 202.7 565.3 192 552 192L512 192C512 156.7 483.3 128 448 128L448 88C448 74.7 437.3 64 424 64C410.7 64 400 74.7 400 88L400 128L344 128L344 88C344 74.7 333.3 64 320 64C306.7 64 296 74.7 296 88L296 128L240 128L240 88zM224 192L416 192C433.7 192 448 206.3 448 224L448 416C448 433.7 433.7 448 416 448L224 448C206.3 448 192 433.7 192 416L192 224C192 206.3 206.3 192 224 192zM240 240L240 400L400 400L400 240L240 240z"
          />
        </svg>
      </div>
      <span class="titlebar-title">{{ t('app.title') }}</span>
      <span class="titlebar-version text-muted">v0.1.0-alpha.2</span>
      <div class="titlebar-spacer" />
      <div class="wc-group">
        <button class="wc-btn wc-minimize" :title="t('app.minimize')" @click="minimizeWindow">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
        <button class="wc-btn wc-maximize" :title="t('app.maximize')" @click="maximizeWindow">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <rect
              x1="1.5"
              y1="1.5"
              width="7"
              height="7"
              rx="1"
              stroke="currentColor"
              stroke-width="1.5"
              fill="none"
            />
          </svg>
        </button>
        <button class="wc-btn wc-close" :title="t('app.close')" @click="closeWindow">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.5" />
            <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>
    </header>

    <ToolBar />

    <div class="app-body">
      <aside class="sidebar">
        <OperationPanel />
      </aside>
      <div class="sidebar-divider" />
      <main class="content">
        <div class="hex-pane">
          <div class="pane-bar">
            <svg
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect x="2" y="3" width="20" height="18" rx="2" />
              <line x1="8" y1="3" x2="8" y2="21" />
              <line x1="14" y1="3" x2="14" y2="21" />
              <line x1="2" y1="9" x2="22" y2="9" />
              <line x1="2" y1="15" x2="22" y2="15" />
            </svg>
            <span class="pane-bar-title">{{ t('pane.hexView') }}</span>
            <div class="pane-bar-spacer" />
          </div>
          <div class="pane-body">
            <HexViewer :data="store.hexData" :base-addr="0" />
          </div>
        </div>

        <div class="resize-handle" :title="t('app.dragToResize')" @mousedown="onDividerMouseDown">
          <div class="resize-grip" />
        </div>

        <div class="log-pane" :style="{ height: logHeight + 'px' }">
          <div class="pane-bar">
            <svg
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <polyline points="4 17 10 11 4 5" />
              <line x1="12" y1="19" x2="20" y2="19" />
            </svg>
            <span class="pane-bar-title">{{ t('pane.outputLog') }}</span>
            <div class="pane-bar-spacer" />
            <button
              class="btn btn-ghost btn-sm"
              :data-tooltip="t('action.clearLog')"
              @click="store.clearLogs()"
            >
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
              </svg>
            </button>
          </div>
          <div class="pane-body">
            <LogConsole :logs="store.logs" />
          </div>
        </div>
      </main>
    </div>

    <StatusBar />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg-base);
  border-radius: 12px;
}
.app-shell.is-resizing {
  cursor: ns-resize;
  user-select: none;
}

.titlebar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 14px;
  height: 40px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  -webkit-app-region: drag;
}
.titlebar button {
  -webkit-app-region: no-drag;
}
.titlebar-icon {
  color: var(--accent);
  display: flex;
  align-items: center;
}
.titlebar-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.01em;
  color: var(--text-primary);
}
.titlebar-version {
  font-family: var(--font-mono);
  font-size: 10px;
}
.titlebar-spacer {
  flex: 1;
}

.wc-group {
  display: flex;
  align-items: center;
  gap: 2px;
  -webkit-app-region: no-drag;
  margin-right: -8px;
}
.wc-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition:
    background 120ms,
    color 120ms;
}
.wc-btn:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.wc-close:hover {
  background: #e81123;
  color: #fff;
}

.app-body {
  display: flex;
  flex: 1;
  overflow: hidden;
  min-height: 0;
}
.sidebar {
  width: 230px;
  flex-shrink: 0;
  overflow-y: auto;
  background: var(--bg-surface);
}
.sidebar-divider {
  width: 1px;
  background: var(--border);
  flex-shrink: 0;
}
.content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}

.pane-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  height: 32px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  color: var(--text-muted);
}
.pane-bar-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.01em;
  text-transform: uppercase;
  color: var(--text-secondary);
}
.pane-bar-spacer {
  flex: 1;
}

.hex-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}
.pane-body {
  flex: 1;
  overflow: hidden;
}

.resize-handle {
  height: 5px;
  flex-shrink: 0;
  background: var(--bg-surface);
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  cursor: ns-resize;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 120ms;
}
.resize-handle:hover {
  background: var(--bg-elevated);
}
.resize-grip {
  width: 32px;
  height: 2px;
  border-radius: 1px;
  background: var(--border);
}

.log-pane {
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  overflow: hidden;
}
</style>
