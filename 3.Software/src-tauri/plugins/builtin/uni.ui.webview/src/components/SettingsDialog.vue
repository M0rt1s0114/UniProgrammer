<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { useSettingsStore, type ThemeMode } from '@/stores/settings'
import { useProgStore } from '@/stores/prog'
import PluginManagerDialog from '@/components/PluginManagerDialog.vue'
import type { Locale } from '@/i18n'
import { t } from '@/i18n'

const settings = useSettingsStore()
const store = useProgStore()
const open = defineModel<boolean>('open', { default: false })

const showVccConfirm = ref(false)
const pluginManagerOpen = ref(false)

// 对话框内先编辑草稿，点击“确定”后才写入 settings store；
// 点取消 / 点击遮罩关闭则丢弃本次修改。
const draft = reactive({
  language: 'zh' as Locale,
  theme: 'dark' as ThemeMode,
  debugConsole: false,
  batchBurn: false,
  saveVoltage: false,
  powerAutoDetect: false,
  autoDetectEeprom: false,
  progressEstimate: false,
  checkSoundSwitch: true,
  blankCheckAfterErase: false,
  vccControlEnabled: false,
})

function loadDraft() {
  draft.language = settings.language
  draft.theme = settings.theme
  draft.debugConsole = settings.debugConsole
  draft.batchBurn = settings.batchBurn
  draft.saveVoltage = settings.saveVoltage
  draft.powerAutoDetect = settings.powerAutoDetect
  draft.autoDetectEeprom = settings.autoDetectEeprom
  draft.progressEstimate = settings.progressEstimate
  draft.checkSoundSwitch = settings.checkSoundSwitch
  draft.blankCheckAfterErase = settings.blankCheckAfterErase
  draft.vccControlEnabled = settings.vccControlEnabled
  showVccConfirm.value = false
}

watch(
  () => open.value,
  (isOpen) => {
    if (isOpen) loadDraft()
  },
)

function applyDraft() {
  settings.language = draft.language
  settings.theme = draft.theme
  settings.debugConsole = draft.debugConsole
  settings.batchBurn = draft.batchBurn
  settings.saveVoltage = draft.saveVoltage
  settings.powerAutoDetect = draft.powerAutoDetect
  settings.autoDetectEeprom = draft.autoDetectEeprom
  settings.progressEstimate = draft.progressEstimate
  settings.checkSoundSwitch = draft.checkSoundSwitch
  settings.blankCheckAfterErase = draft.blankCheckAfterErase
  settings.vccControlEnabled = draft.vccControlEnabled
}

function confirm() {
  applyDraft()
  pluginManagerOpen.value = false
  open.value = false
}

function cancel() {
  showVccConfirm.value = false
  pluginManagerOpen.value = false
  open.value = false
}

// 开启电压控制总开关需要黄色确认框（无需输入）；正在输出电压时禁止关闭总开关
function onVccControlChange(event: Event) {
  const target = (event.target as HTMLInputElement).checked
  if (!target) {
    draft.vccControlEnabled = false
    return
  }
  showVccConfirm.value = true
}

function confirmVccControl() {
  showVccConfirm.value = false
  draft.vccControlEnabled = true
}

function cancelVccControl() {
  showVccConfirm.value = false
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="modal-backdrop" @click.self="cancel">
      <div class="modal settings-modal">
        <h3 class="modal-title">{{ t('app.settings') }}</h3>

        <div class="settings-body">
          <div class="settings-section">
            <div class="settings-label">{{ t('settings.autoTitle') }}</div>
            <div class="settings-grid">
              <label class="toggle-row">
                <input v-model="draft.batchBurn" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.batchBurn') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.saveVoltage" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.saveVoltage') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.powerAutoDetect" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.powerAutoDetect') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.autoDetectEeprom" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.autoDetectEeprom') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.progressEstimate" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.progressEstimate') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.checkSoundSwitch" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.checkSoundSwitch') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.blankCheckAfterErase" type="checkbox" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.blankCheckAfterErase') }}</span>
              </label>
            </div>
          </div>

          <div class="settings-section">
            <label class="toggle-row settings-master">
              <input
                type="checkbox"
                class="toggle-check"
                :checked="draft.vccControlEnabled"
                :disabled="store.vccOutputEnabled"
                @change="onVccControlChange"
              />
              <span class="toggle-text">{{ t('settings.vccControl') }}</span>
            </label>
            <div v-if="store.vccOutputEnabled" class="settings-hint settings-hint-danger">
              {{ t('settings.vccBusyHint') }}
            </div>
          </div>

          <div class="settings-section">
            <div class="settings-label">{{ t('settings.language') }}</div>
            <div class="settings-radio-row">
              <label class="toggle-row">
                <input v-model="draft.language" type="radio" value="zh" class="toggle-check" />
                <span class="toggle-text">中文</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.language" type="radio" value="en" class="toggle-check" />
                <span class="toggle-text">English</span>
              </label>
            </div>
          </div>

          <div class="settings-section">
            <div class="settings-label">{{ t('settings.theme') }}</div>
            <div class="settings-radio-row">
              <label class="toggle-row">
                <input v-model="draft.theme" type="radio" value="dark" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.themeDark') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.theme" type="radio" value="light" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.themeLight') }}</span>
              </label>
              <label class="toggle-row">
                <input v-model="draft.theme" type="radio" value="system" class="toggle-check" />
                <span class="toggle-text">{{ t('settings.themeSystem') }}</span>
              </label>
            </div>
          </div>

          <div class="settings-section">
            <label class="toggle-row">
              <input v-model="draft.debugConsole" type="checkbox" class="toggle-check" />
              <span class="toggle-text">{{ t('settings.debugConsole') }}</span>
            </label>
            <div class="settings-hint">{{ t('settings.debugConsoleHint') }}</div>
          </div>

          <div class="settings-section">
            <button class="btn btn-secondary" style="width: 100%" @click="pluginManagerOpen = true">
              {{ t('pluginManager.open') }}
            </button>
          </div>
        </div>

        <div class="modal-actions">
          <button class="btn btn-secondary" @click="cancel">{{ t('action.cancel') }}</button>
          <button class="btn btn-primary" @click="confirm">{{ t('action.confirm') }}</button>
        </div>
      </div>
    </div>
  </Transition>

  <PluginManagerDialog v-model:open="pluginManagerOpen" />

  <!-- 电压控制总开关确认（黄色，无需输入） -->
  <Transition name="fade">
    <div
      v-if="showVccConfirm"
      class="modal-backdrop settings-confirm-backdrop"
      @click.self="cancelVccControl"
    >
      <div class="modal modal-warn">
        <div class="modal-icon">
          <svg
            width="22"
            height="22"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path
              d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"
            />
            <line x1="12" y1="9" x2="12" y2="13" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
        </div>
        <h3 class="modal-title">{{ t('vcc.masterTitle') }}</h3>
        <p class="modal-body">{{ t('vcc.masterBody') }}</p>
        <div class="modal-actions">
          <button class="btn btn-secondary" @click="cancelVccControl">
            {{ t('action.cancel') }}
          </button>
          <button class="btn btn-warn" @click="confirmVccControl">
            {{ t('vcc.masterEnable') }}
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.settings-modal {
  max-width: 500px;
  text-align: left;
}
.settings-body {
  max-height: 60vh;
  overflow-y: auto;
  padding-right: 4px;
}
.settings-section {
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.settings-section + .settings-section {
  margin-top: 10px;
}
.settings-label {
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.settings-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px 12px;
}
.settings-master {
  padding: 4px;
  border: 1px solid var(--warn-border);
  background: var(--warn-soft);
  border-radius: var(--radius-md);
}
.settings-hint {
  margin-top: 6px;
  font-size: 11px;
  color: var(--text-muted);
}
.settings-hint-danger {
  color: var(--color-danger);
}
.settings-radio-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.settings-confirm-backdrop {
  z-index: 220;
}
</style>
