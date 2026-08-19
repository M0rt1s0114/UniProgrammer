<script setup lang="ts">
import { ref, watch } from 'vue'
import { useProgStore } from '@/stores/prog'
import { useSettingsStore } from '@/stores/settings'
import { useSpiNor } from '@/services/spiNor'
import {
  disablePlugin,
  enablePlugin,
  eraseSidecarChip,
  listBuiltinModules,
  listPlugins,
  listSidecarAdapters,
  readSidecarChip,
  readSidecarId,
  selectSidecarAdapter,
  unselectSidecarAdapter,
  verifySidecarChip,
  writeSidecarChip,
  type BuiltinModule,
  type PluginInfo,
  type SidecarAdapterEntry,
} from '@/services/plugins'
import { onProgress } from '@/services/ipc'
import { t } from '@/i18n'

const store = useProgStore()
const settings = useSettingsStore()
const spiNor = useSpiNor()
const open = defineModel<boolean>('open', { default: false })

const SIDECAR_SEP = '\u0000'
const sidecarAdapters = ref<SidecarAdapterEntry[]>([])
const sidecarSelected = ref('')
const sidecarBusy = ref(false)
const sidecarReadSize = ref(1048576)
const sidecarOp = ref('')

const plugins = ref<PluginInfo[]>([])
const builtinModules = ref<BuiltinModule[]>([])
const pluginsLoading = ref(false)
const pluginActionName = ref('')

function parseSidecarSelection(value: string): { adapter: string; device: string } | null {
  const sep = value.indexOf(SIDECAR_SEP)
  if (!value || sep <= 0) return null
  return { adapter: value.slice(0, sep), device: value.slice(sep + SIDECAR_SEP.length) }
}

async function refreshSidecarAdapters() {
  sidecarBusy.value = true
  try {
    sidecarAdapters.value = await listSidecarAdapters()
    sidecarSelected.value = ''
    store.addLog(`Sidecar 适配器已刷新，共 ${sidecarAdapters.value.length} 个适配器`)
  } catch (error) {
    store.addLog(`刷新 Sidecar 适配器失败: ${String(error)}`, 'error')
  } finally {
    sidecarBusy.value = false
  }
}

async function sidecarReadId() {
  const selection = parseSidecarSelection(sidecarSelected.value)
  if (!selection) {
    store.addLog('未选择 Sidecar 设备', 'warn')
    return
  }
  sidecarBusy.value = true
  try {
    const msg = await readSidecarId(selection.adapter, selection.device)
    store.addLog(msg, 'success')
  } catch (error) {
    store.addLog(`读取 Sidecar 芯片 ID 失败: ${String(error)}`, 'error')
  } finally {
    sidecarBusy.value = false
  }
}

async function sidecarSelect() {
  const selection = parseSidecarSelection(sidecarSelected.value)
  if (!selection) {
    store.addLog('未选择 Sidecar 设备', 'warn')
    return
  }
  sidecarBusy.value = true
  try {
    const msg = await selectSidecarAdapter(selection.adapter, selection.device)
    store.status = 'success'
    store.connectedDevice = `Sidecar · ${selection.adapter} / ${selection.device}`
    store.addLog(msg, 'success')
    await spiNor.detectChip()
  } catch (error) {
    store.addLog(`选定 Sidecar 编程器失败: ${String(error)}`, 'error')
  } finally {
    sidecarBusy.value = false
  }
}

async function sidecarUnselect() {
  sidecarBusy.value = true
  try {
    await unselectSidecarAdapter()
    store.addLog('已取消选择 Sidecar 编程器', 'success')
    if (store.connectedDevice.startsWith('Sidecar ·')) {
      store.status = 'error'
      store.connectedDevice = ''
      store.chipDetected = false
      store.detectedChipSize = 0
      store.chipDetails = null
    }
  } catch (error) {
    store.addLog(`取消选择 Sidecar 编程器失败: ${String(error)}`, 'error')
  } finally {
    sidecarBusy.value = false
  }
}

function selectedSidecarTarget(): { adapter: string; device: string } | null {
  const selection = parseSidecarSelection(sidecarSelected.value)
  if (!selection) {
    store.addLog('未选择 Sidecar 设备', 'warn')
    return null
  }
  return selection
}

async function sidecarErase() {
  const selection = selectedSidecarTarget()
  if (!selection) return
  sidecarBusy.value = true
  sidecarOp.value = t('sidecar.erase')
  try {
    const msg = await eraseSidecarChip(selection.adapter, selection.device)
    store.addLog(msg, 'success')
  } catch (error) {
    store.addLog(`擦除 Sidecar 芯片失败: ${String(error)}`, 'error')
  } finally {
    sidecarBusy.value = false
    sidecarOp.value = ''
  }
}

async function sidecarRead() {
  const selection = selectedSidecarTarget()
  if (!selection) return
  sidecarBusy.value = true
  sidecarOp.value = t('sidecar.read')
  let unlisten: (() => void) | null = null
  store.progress = 0
  store.progressMessage = `${t('sidecar.read')}...`
  try {
    unlisten = await onProgress<{ done: number; total: number }>('read_progress', (payload) => {
      store.progress = payload.total > 0 ? Math.round((payload.done / payload.total) * 100) : 0
      store.progressMessage = `${t('sidecar.read')}... ${store.progress}%`
    })
    const buf = await readSidecarChip(selection.adapter, selection.device, sidecarReadSize.value)
    store.hexData = new Uint8Array(buf)
    store.detectedChipSize = buf.byteLength
    store.progress = 100
    store.progressMessage = '读取完成'
    store.addLog(`读取完成，共 ${buf.byteLength} 字节`, 'success')
  } catch (error) {
    store.addLog(`读取 Sidecar 芯片失败: ${String(error)}`, 'error')
    store.progress = 0
    store.progressMessage = ''
  } finally {
    if (unlisten) unlisten()
    sidecarBusy.value = false
    sidecarOp.value = ''
  }
}

async function sidecarWrite() {
  const selection = selectedSidecarTarget()
  if (!selection) return
  const payload = store.hexData
  if (!payload || payload.length === 0) {
    store.addLog('未加载数据，无法写入 Sidecar 芯片', 'warn')
    return
  }
  sidecarBusy.value = true
  sidecarOp.value = t('sidecar.write')
  let unlisten: (() => void) | null = null
  store.progress = 0
  store.progressMessage = `${t('sidecar.write')}...`
  try {
    unlisten = await onProgress<{ done: number; total: number }>('write_progress', (payload) => {
      store.progress = payload.total > 0 ? Math.round((payload.done / payload.total) * 100) : 0
      store.progressMessage = `${t('sidecar.write')}... ${store.progress}%`
    })
    const msg = await writeSidecarChip(selection.adapter, selection.device, payload)
    store.progress = 100
    store.progressMessage = '写入完成'
    store.addLog(msg, 'success')
  } catch (error) {
    store.addLog(`写入 Sidecar 芯片失败: ${String(error)}`, 'error')
    store.progress = 0
    store.progressMessage = ''
  } finally {
    if (unlisten) unlisten()
    sidecarBusy.value = false
    sidecarOp.value = ''
  }
}

async function sidecarVerify() {
  const selection = selectedSidecarTarget()
  if (!selection) return
  const payload = store.hexData
  if (!payload || payload.length === 0) {
    store.addLog('未加载数据，无法校验 Sidecar 芯片', 'warn')
    return
  }
  sidecarBusy.value = true
  sidecarOp.value = t('sidecar.verify')
  let unlisten: (() => void) | null = null
  store.progress = 0
  store.progressMessage = `${t('sidecar.verify')}...`
  try {
    unlisten = await onProgress<{ done: number; total: number }>('verify_progress', (payload) => {
      store.progress = payload.total > 0 ? Math.round((payload.done / payload.total) * 100) : 0
      store.progressMessage = `${t('sidecar.verify')}... ${store.progress}%`
    })
    const msg = await verifySidecarChip(selection.adapter, selection.device, payload)
    store.progress = 100
    store.progressMessage = '校验完成'
    store.addLog(msg, 'success')
  } catch (error) {
    store.addLog(`校验 Sidecar 芯片失败: ${String(error)}`, 'error')
    store.progress = 0
    store.progressMessage = ''
  } finally {
    if (unlisten) unlisten()
    sidecarBusy.value = false
    sidecarOp.value = ''
  }
}

async function loadPlugins() {
  pluginsLoading.value = true
  try {
    plugins.value = await listPlugins()
  } catch (error) {
    store.addLog(`加载插件列表失败: ${String(error)}`, 'error')
  } finally {
    pluginsLoading.value = false
  }
}

async function loadBuiltinModules() {
  try {
    builtinModules.value = await listBuiltinModules()
  } catch (error) {
    store.addLog(`加载内置模块失败: ${String(error)}`, 'error')
  }
}

async function setPluginEnabled(plugin: PluginInfo, enabled: boolean) {
  if (store.isRunning || pluginActionName.value) return
  pluginActionName.value = plugin.name
  try {
    const msg = enabled ? await enablePlugin(plugin.name) : await disablePlugin(plugin.name)
    store.addLog(msg, 'success')
    await loadPlugins()
  } catch (error) {
    store.addLog(`${enabled ? '启用' : '禁用'}插件失败: ${String(error)}`, 'error')
  } finally {
    pluginActionName.value = ''
  }
}

function close() {
  open.value = false
}

watch(
  () => open.value,
  (isOpen) => {
    if (isOpen) {
      void loadPlugins()
      void loadBuiltinModules()
      // Sidecar 面板打开时自动刷新；默认隐藏，在设置中开启。
      if (settings.showSidecarPanel) {
        void refreshSidecarAdapters()
      }
    }
  },
)
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="modal-backdrop plugin-manager-backdrop" @click.self="close">
      <div class="modal plugin-manager-modal">
        <h3 class="modal-title">{{ t('pluginManager.title') }}</h3>

        <div class="plugin-manager-body">
          <section class="plugin-section">
            <div class="plugin-section-label">{{ t('pluginManager.installed') }}</div>

            <div v-if="pluginsLoading" class="plugin-hint">...</div>
            <div v-else-if="plugins.length === 0" class="plugin-hint">—</div>

            <div v-for="plugin in plugins" :key="plugin.name" class="plugin-item">
              <div class="plugin-info">
                <div class="plugin-name">
                  {{ plugin.name }}
                  <span class="plugin-version">v{{ plugin.version }}</span>
                </div>
                <div class="plugin-meta">
                  {{ plugin.kind }}
                  <span class="plugin-badge plugin-badge-off">
                    {{ t(`pluginManager.layer.${plugin.layer}`) }}
                  </span>
                  <span
                    class="plugin-badge"
                    :class="plugin.enabled ? 'plugin-badge-on' : 'plugin-badge-off'"
                  >
                    {{ plugin.enabled ? t('common.yes') : t('common.no') }}
                  </span>
                </div>
                <div v-if="plugin.error" class="plugin-error">{{ plugin.error }}</div>
              </div>
              <button
                v-if="plugin.layer !== 'required'"
                class="btn btn-secondary btn-sm"
                :disabled="store.isRunning || pluginActionName !== ''"
                @click="setPluginEnabled(plugin, !plugin.enabled)"
              >
                {{ plugin.enabled ? t('pluginManager.disable') : t('pluginManager.enable') }}
              </button>
              <span v-else class="plugin-required-hint">{{ t('pluginManager.requiredHint') }}</span>
            </div>
          </section>

          <details class="plugin-builtin">
            <summary>{{ t('pluginManager.builtin') }}</summary>
            <div v-if="builtinModules.length === 0" class="plugin-hint">—</div>
            <div v-for="mod in builtinModules" :key="mod.name" class="builtin-item">
              <span class="builtin-name">{{ mod.name }} v{{ mod.version }}</span>
              <span class="builtin-desc">{{ mod.description }}</span>
            </div>
          </details>

          <section v-if="settings.showSidecarPanel" class="plugin-section">
            <div class="plugin-section-label">{{ t('sidecar.title') }}</div>

            <button
              class="btn btn-ghost btn-sm w-full"
              :disabled="sidecarBusy"
              @click="refreshSidecarAdapters"
            >
              {{ t('sidecar.refresh') }}
            </button>

            <select
              v-model="sidecarSelected"
              class="input"
              style="margin-top: 6px"
              :disabled="sidecarBusy"
            >
              <template v-for="adapter in sidecarAdapters" :key="adapter.name">
                <option
                  v-for="device in adapter.devices"
                  :key="`${adapter.name}\u0000${device.id}`"
                  :value="`${adapter.name}\u0000${device.id}`"
                >
                  {{ adapter.name }} · {{ device.id }} ({{ device.detail }})
                </option>
              </template>
            </select>

            <div style="display: flex; gap: 6px; margin-top: 6px">
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy || !sidecarSelected"
                @click="sidecarSelect"
              >
                {{ t('sidecar.select') }}
              </button>
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy"
                @click="sidecarUnselect"
              >
                {{ t('sidecar.unselect') }}
              </button>
            </div>

            <button
              class="btn btn-secondary w-full"
              style="margin-top: 6px"
              :disabled="sidecarBusy || !sidecarSelected"
              @click="sidecarReadId"
            >
              {{ t('sidecar.readId') }}
            </button>

            <div class="field" style="margin-top: 6px">
              <label class="field-label">{{ t('sidecar.readSize') }}</label>
              <input
                v-model.number="sidecarReadSize"
                type="number"
                min="1"
                max="0x1000000"
                class="input"
              />
            </div>

            <div style="display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px">
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy || !sidecarSelected"
                @click="sidecarErase"
              >
                {{ t('sidecar.erase') }}
              </button>
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy || !sidecarSelected"
                @click="sidecarRead"
              >
                {{ t('sidecar.read') }}
              </button>
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy || !sidecarSelected"
                @click="sidecarWrite"
              >
                {{ t('sidecar.write') }}
              </button>
              <button
                class="btn btn-secondary btn-sm"
                style="flex: 1"
                :disabled="sidecarBusy || !sidecarSelected"
                @click="sidecarVerify"
              >
                {{ t('sidecar.verify') }}
              </button>
            </div>

            <div v-if="sidecarOp" class="field-hint" style="margin-top: 4px">
              {{ sidecarOp }}
            </div>
          </section>

          <div v-if="store.isRunning" class="plugin-hint plugin-hint-danger">
            {{ t('pluginManager.runningDisabled') }}
          </div>
        </div>

        <div class="modal-actions">
          <button class="btn btn-secondary" @click="close">{{ t('pluginManager.close') }}</button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.plugin-manager-backdrop {
  z-index: 210;
}
.plugin-manager-modal {
  max-width: 560px;
  width: calc(100vw - 40px);
  text-align: left;
}
.plugin-manager-body {
  max-height: 65vh;
  overflow-y: auto;
  padding-right: 4px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.plugin-section {
  border-top: 1px solid var(--border);
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.plugin-section-label {
  font-size: 11px;
  color: var(--text-secondary);
  font-weight: 600;
}
.plugin-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-base);
}
.plugin-info {
  flex: 1;
  min-width: 0;
}
.plugin-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: var(--font-sans);
}
.plugin-version {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-muted);
  margin-left: 4px;
}
.plugin-meta {
  margin-top: 2px;
  font-size: 11px;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 6px;
}
.plugin-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 99px;
  font-size: 10px;
  border: 1px solid var(--border);
}
.plugin-badge-on {
  color: var(--accent);
  border-color: var(--border-accent);
  background: var(--accent-subtle);
}
.plugin-badge-off {
  color: var(--text-muted);
  background: var(--bg-elevated);
}
.plugin-error {
  margin-top: 2px;
  font-size: 11px;
  color: var(--color-danger);
  font-family: var(--font-sans);
}
.plugin-required-hint {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}
.plugin-hint {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-sans);
}
.plugin-hint-danger {
  color: var(--color-danger);
}
.plugin-builtin {
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.plugin-builtin summary {
  cursor: pointer;
  font-size: 11px;
  color: var(--text-secondary);
  font-weight: 600;
  user-select: none;
}
.builtin-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  margin-top: 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-base);
}
.builtin-name {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: var(--font-mono);
}
.builtin-desc {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-sans);
}
</style>
