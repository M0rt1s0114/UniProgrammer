<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useProgStore } from '@/stores/prog'
import { useSettingsStore } from '@/stores/settings'
import { useSpiNor } from '@/services/spiNor'
import { t } from '@/i18n'
import SettingsDialog from '@/components/SettingsDialog.vue'
import AboutDialog from '@/components/AboutDialog.vue'

const store = useProgStore()
const settings = useSettingsStore()
const spiNor = useSpiNor()

// Toolbar icons supplied by the project owner.
// All four operation icons are Font Awesome Free 7.3.1 (CC BY 4.0).

const showEraseConfirm = ref(false)
const showAutoConfirm = ref(false)
const showAutoConfig = ref(false)
const showSettings = ref(false)
const showAbout = ref(false)

const AUTO_STEP_KEYS = ['read', 'erase', 'blankCheck', 'write', 'verify'] as const
type AutoStepKey = (typeof AUTO_STEP_KEYS)[number]
const autoStepLabels: Record<AutoStepKey, string> = {
  read: t('auto.stepRead'),
  erase: t('auto.stepErase'),
  blankCheck: t('auto.stepBlankCheck'),
  write: t('auto.stepWrite'),
  verify: t('auto.stepVerify'),
}

function parseAutoOrder(value: string): AutoStepKey[] {
  const valid = new Set<string>(AUTO_STEP_KEYS)
  return value
    .split(',')
    .map((step) => step.trim())
    .filter((step): step is AutoStepKey => valid.has(step))
}

interface AutoEntry {
  uid: number
  step: AutoStepKey
}

let autoEntryUid = 0
function entriesFromOrder(value: string): AutoEntry[] {
  return parseAutoOrder(value).map((step) => ({ uid: ++autoEntryUid, step }))
}

// 设置弹窗内部使用草稿；点“保存”才写回 settings，“关闭”直接丢弃。
const draftAutoEntries = ref<AutoEntry[]>([])
const draftAutoSteps = computed<AutoStepKey[]>(() => draftAutoEntries.value.map((entry) => entry.step))
const savedAutoSteps = computed<AutoStepKey[]>(() => parseAutoOrder(settings.autoOrder))
const availableAutoSteps = computed<AutoStepKey[]>(() => [...AUTO_STEP_KEYS])
const allAutoStepsUsed = computed(
  () => new Set(draftAutoSteps.value).size === AUTO_STEP_KEYS.length,
)
const draftAutoStepSummary = computed(() =>
  draftAutoSteps.value.map((step) => autoStepLabels[step]).join(' → '),
)
const savedAutoStepSummary = computed(() =>
  savedAutoSteps.value.map((step) => autoStepLabels[step]).join(' → '),
)

watch(
  () => showAutoConfig.value,
  (open) => {
    stopAutoDrag()
    if (open) {
      draftAutoEntries.value = entriesFromOrder(settings.autoOrder)
    }
  },
)

function addAutoStep(step: AutoStepKey) {
  draftAutoEntries.value = [...draftAutoEntries.value, { uid: ++autoEntryUid, step }]
}

function removeAutoStep(index: number) {
  const entries = [...draftAutoEntries.value]
  entries.splice(index, 1)
  draftAutoEntries.value = entries
}

function moveAutoStep(index: number, delta: -1 | 1) {
  const target = index + delta
  if (target < 0 || target >= draftAutoEntries.value.length) return
  reorderAutoStep(index, target)
}

function reorderAutoStep(from: number, target: number) {
  if (from === target) return
  const entries = [...draftAutoEntries.value]
  const [entry] = entries.splice(from, 1)
  entries.splice(target, 0, entry)
  draftAutoEntries.value = entries
}

function saveAutoConfig() {
  settings.autoOrder = draftAutoSteps.value.join(',')
  showAutoConfig.value = false
}

function closeAutoConfig() {
  showAutoConfig.value = false
}

const draggedAutoIndex = ref<number | null>(null)
const dropTargetIndex = ref<number | null>(null)
let autoDragCleanup: (() => void) | null = null

function stopAutoDrag() {
  autoDragCleanup?.()
  autoDragCleanup = null
  draggedAutoIndex.value = null
  dropTargetIndex.value = null
  document.body.style.userSelect = ''
}

function startAutoDrag(index: number, event: PointerEvent) {
  if (autoDragCleanup) return
  event.preventDefault()
  draggedAutoIndex.value = index
  dropTargetIndex.value = null
  document.body.style.userSelect = 'none'

  const onMove = (moveEvent: PointerEvent) => {
    const items = Array.from(document.querySelectorAll<HTMLElement>('.auto-order-item'))
    if (items.length < 2) return
    let best = -1
    let bestDistance = Number.POSITIVE_INFINITY
    items.forEach((item, itemIndex) => {
      const rect = item.getBoundingClientRect()
      const centerY = rect.top + rect.height / 2
      const distance = Math.abs(moveEvent.clientY - centerY)
      if (distance < bestDistance) {
        bestDistance = distance
        best = itemIndex
      }
    })
    dropTargetIndex.value = best === draggedAutoIndex.value ? null : best
  }
  const onStop = () => {
    const from = draggedAutoIndex.value
    const target = dropTargetIndex.value
    stopAutoDrag()
    if (from !== null && target !== null) {
      reorderAutoStep(from, target)
    }
  }

  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onStop, { once: true })
  window.addEventListener('pointercancel', onStop, { once: true })
  autoDragCleanup = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onStop)
    window.removeEventListener('pointercancel', onStop)
  }
}

function requestErase() {
  showEraseConfirm.value = true
}

function requestAuto() {
  if (savedAutoSteps.value.includes('erase') || savedAutoSteps.value.includes('write')) {
    showAutoConfirm.value = true
    return
  }
  // 空流程不弹设置框，只由 runAuto 输出“未设置自动化流程”
  void spiNor.runAuto()
}

function confirmAuto() {
  showAutoConfirm.value = false
  void spiNor.runAuto()
}

async function confirmErase() {
  showEraseConfirm.value = false
  await spiNor.eraseChip()
}
</script>

<template>
  <div class="toolbar">
    <div class="tool-group">
      <button class="tool-btn" :title="t('action.openFile')" @click="store.openFileViaDialog()">
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
            <path d="M3 10h18" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.openFile') }}</span>
      </button>

      <button
        class="tool-btn"
        :title="t('action.saveBin')"
        :disabled="!store.hexData"
        @click="spiNor.saveFileNative('bin')"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M5 3h11l3 3v15H5z" />
            <path d="M8 3v6h8V3" />
            <path d="M9 13h6M9 17h6" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.saveBin') }}</span>
      </button>

      <button
        class="tool-btn"
        :title="t('action.saveHex')"
        :disabled="!store.hexData"
        @click="spiNor.saveFileNative('hex')"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M6 2h9l4 4v16H6z" />
            <path d="M14 2v5h5" />
            <path d="M9 12h6M9 16h6" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.saveHex') }}</span>
      </button>
    </div>

    <div class="toolbar-divider" />

    <div class="tool-group tool-group-ops">
      <button
        class="tool-btn read-icon"
        :title="t('action.read')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.readChip()"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M352 96C352 78.3 337.7 64 320 64C302.3 64 288 78.3 288 96L288 306.7L246.6 265.3C234.1 252.8 213.8 252.8 201.3 265.3C188.8 277.8 188.8 298.1 201.3 310.6L297.3 406.6C309.8 419.1 330.1 419.1 342.6 406.6L438.6 310.6C451.1 298.1 451.1 277.8 438.6 265.3C426.1 252.8 405.8 252.8 393.3 265.3L352 306.7L352 96zM160 384C124.7 384 96 412.7 96 448L96 480C96 515.3 124.7 544 160 544L480 544C515.3 544 544 515.3 544 480L544 448C544 412.7 515.3 384 480 384L433.1 384L376.5 440.6C345.3 471.8 294.6 471.8 263.4 440.6L206.9 384L160 384zM464 440C477.3 440 488 450.7 488 464C488 477.3 477.3 488 464 488C450.7 488 440 477.3 440 464C440 450.7 450.7 440 464 440z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.read') }}</span>
      </button>

      <button
        class="tool-btn write-icon"
        :title="t('action.write')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.writeChip()"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M352 173.3L352 384C352 401.7 337.7 416 320 416C302.3 416 288 401.7 288 384L288 173.3L246.6 214.7C234.1 227.2 213.8 227.2 201.3 214.7C188.8 202.2 188.8 181.9 201.3 169.4L297.3 73.4C309.8 60.9 330.1 60.9 342.6 73.4L438.6 169.4C451.1 181.9 451.1 202.2 438.6 214.7C426.1 227.2 405.8 227.2 393.3 214.7L352 173.3zM320 464C364.2 464 400 428.2 400 384L480 384C515.3 384 544 412.7 544 448L544 480C544 515.3 515.3 544 480 544L160 544C124.7 544 96 515.3 96 480L96 448C96 412.7 124.7 384 160 384L240 384C240 428.2 275.8 464 320 464zM464 488C477.3 488 488 477.3 488 464C488 450.7 477.3 440 464 440C450.7 440 440 450.7 440 464C440 477.3 450.7 488 464 488z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.write') }}</span>
      </button>

      <button
        class="tool-btn erase-icon"
        :title="t('action.erase')"
        :disabled="!store.canOperate || store.isRunning"
        @click="requestErase"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M210.5 480L333.5 480L398.8 414.7L225.3 241.2L98.6 367.9L210.6 479.9zM256 544L210.5 544C193.5 544 177.2 537.3 165.2 525.3L49 409C38.1 398.1 32 383.4 32 368C32 352.6 38.1 337.9 49 327L295 81C305.9 70.1 320.6 64 336 64C351.4 64 366.1 70.1 377 81L559 263C569.9 273.9 576 288.6 576 304C576 319.4 569.9 334.1 559 345L424 480L544 480C561.7 480 576 494.3 576 512C576 529.7 561.7 544 544 544L256 544z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.erase') }}</span>
      </button>

      <button
        class="tool-btn verify-icon"
        :title="t('action.verify')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.verifyChip()"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M530.8 134.1C545.1 144.5 548.3 164.5 537.9 178.8L281.9 530.8C276.4 538.4 267.9 543.1 258.5 543.9C249.1 544.7 240 541.2 233.4 534.6L105.4 406.6C92.9 394.1 92.9 373.8 105.4 361.3C117.9 348.8 138.2 348.8 150.7 361.3L252.2 462.8L486.2 141.1C496.6 126.8 516.6 123.6 530.9 134z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.verify') }}</span>
      </button>

      <button
        class="tool-btn blank-check-icon"
        :title="t('action.blankCheck')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.blankCheckChip()"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M480 96C515.3 96 544 124.7 544 160L544 480C544 515.3 515.3 544 480 544L160 544C124.7 544 96 515.3 96 480L96 160C96 124.7 124.7 96 160 96L480 96zM438 209.7C427.3 201.9 412.3 204.3 404.5 215L285.1 379.2L233 327.1C223.6 317.7 208.4 317.7 199.1 327.1C189.8 336.5 189.7 351.7 199.1 361L271.1 433C276.1 438 283 440.5 289.9 440C296.8 439.5 303.3 435.9 307.4 430.2L443.3 243.2C451.1 232.5 448.7 217.5 438 209.7z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.blankCheck') }}</span>
      </button>

      <button
        class="tool-btn auto-icon"
        :title="t('action.auto')"
        :disabled="!store.canOperate || store.isRunning"
        @click="requestAuto"
      >
        <span class="tool-icon">
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M64 320C64 178.6 178.6 64 320 64C461.4 64 576 178.6 576 320C576 461.4 461.4 576 320 576C178.6 576 64 461.4 64 320zM252.3 211.1C244.7 215.3 240 223.4 240 232L240 408C240 416.7 244.7 424.7 252.3 428.9C259.9 433.1 269.1 433 276.6 428.4L420.6 340.4C427.7 336 432.1 328.3 432.1 319.9C432.1 311.5 427.7 303.8 420.6 299.4L276.6 211.4C269.2 206.9 259.9 206.7 252.3 210.9z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.auto') }}</span>
      </button>

      <button
        class="tool-btn auto-gear"
        :title="t('auto.settings')"
        :disabled="store.isRunning"
        @click="showAutoConfig = true"
      >
        <span class="tool-icon">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path
              d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.08a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h.08a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.08a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('auto.settings') }}</span>
      </button>
    </div>

    <div class="toolbar-spacer" />

    <div class="tool-group tool-group-utility">
      <button class="tool-btn" :title="t('app.settings')" @click="showSettings = true">
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path
              d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.08a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h.08a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.08a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z"
            />
          </svg>
        </span>
        <span class="tool-label">{{ t('app.settings') }}</span>
      </button>

      <button class="tool-btn" :title="t('app.about')" @click="showAbout = true">
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="16" x2="12" y2="12" />
            <line x1="12" y1="8" x2="12.01" y2="8" />
          </svg>
        </span>
        <span class="tool-label">{{ t('app.about') }}</span>
      </button>
    </div>

    <SettingsDialog v-model:open="showSettings" />
    <AboutDialog v-model:open="showAbout" />

    <!-- 擦除确认弹窗 -->
    <Transition name="fade">
      <div v-if="showEraseConfirm" class="modal-backdrop" @click.self="showEraseConfirm = false">
        <div class="modal">
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
          <h3 class="modal-title">{{ t('modal.eraseTitle') }}</h3>
          <p class="modal-body">{{ t('modal.eraseBody') }}</p>
          <div class="modal-actions">
            <button class="btn btn-secondary" @click="showEraseConfirm = false">
              {{ t('action.cancel') }}
            </button>
            <button class="btn btn-danger" @click="confirmErase">
              {{ t('action.confirmErase') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <!-- 自动流程配置弹窗 -->
    <Transition name="fade">
      <div v-if="showAutoConfig" class="modal-backdrop" @click.self="closeAutoConfig">
        <div class="modal">
          <h3 class="modal-title">{{ t('auto.settings') }}</h3>
          <TransitionGroup tag="div" name="auto-order" class="auto-step-list">
            <div v-if="draftAutoSteps.length === 0" key="auto-order-empty" class="auto-order-empty">
              {{ t('auto.emptyHint') }}
            </div>
            <div
              v-for="(entry, index) in draftAutoEntries"
              :key="entry.uid"
              class="auto-order-item"
              :class="{
                'is-dragging': draggedAutoIndex === index,
                'is-drop-target': dropTargetIndex === index,
              }"
            >
              <span
                class="auto-order-handle"
                :title="t('auto.dragHint')"
                @pointerdown.prevent="startAutoDrag(index, $event)"
              >
                ⠿
              </span>
              <span class="auto-order-index">{{ index + 1 }}</span>
              <span class="auto-order-name">{{ autoStepLabels[entry.step] }}</span>
              <button class="auto-order-btn" @click="moveAutoStep(index, -1)">↑</button>
              <button class="auto-order-btn" @click="moveAutoStep(index, 1)">↓</button>
              <button class="auto-order-btn auto-order-remove" @click="removeAutoStep(index)">✕</button>
            </div>
          </TransitionGroup>

          <div class="auto-step-pool">
            <button
              v-for="step in availableAutoSteps"
              :key="step"
              class="btn btn-ghost btn-sm"
              @click="addAutoStep(step)"
            >
              + {{ autoStepLabels[step] }}
            </button>
            <span v-if="allAutoStepsUsed" class="field-hint">
              {{ t('auto.allStepsUsed') }}
            </span>
          </div>
          <p class="auto-step-summary">
            {{ draftAutoStepSummary || t('auto.emptyHint') }}
          </p>
          <div class="modal-actions">
            <button class="btn btn-secondary" @click="closeAutoConfig">
              {{ t('auto.close') }}
            </button>
            <button class="btn btn-primary" @click="saveAutoConfig">
              {{ t('auto.save') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <!-- 自动流程确认弹窗（含擦除/写入时） -->
    <Transition name="fade">
      <div v-if="showAutoConfirm" class="modal-backdrop" @click.self="showAutoConfirm = false">
        <div class="modal modal-danger">
          <h3 class="modal-title">{{ t('auto.confirmTitle') }}</h3>
          <p class="modal-body">{{ t('auto.confirmBody') }}</p>
          <p class="auto-step-summary">{{ savedAutoStepSummary }}</p>
          <div class="modal-actions">
            <button class="btn btn-secondary" @click="showAutoConfirm = false">
              {{ t('action.cancel') }}
            </button>
            <button class="btn btn-danger" @click="confirmAuto">
              {{ t('auto.run') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 58px;
  flex-shrink: 0;
  padding: 0 12px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
}

.tool-group {
  display: flex;
  align-items: center;
  gap: 4px;
}

.toolbar-spacer {
  flex: 1;
}

.toolbar-divider {
  width: 1px;
  height: 28px;
  background: var(--border);
  flex-shrink: 0;
}

.tool-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  min-width: 54px;
  height: 46px;
  padding: 4px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  cursor: pointer;
  transition:
    background 120ms,
    border-color 120ms,
    color 120ms;
}

.tool-btn:hover:not(:disabled) {
  background: var(--bg-elevated);
  border-color: var(--border);
  color: var(--text-primary);
}

.tool-btn:active:not(:disabled) {
  background: var(--bg-overlay);
}

.tool-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.tool-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.tool-label {
  font-size: 10px;
  line-height: 1;
  white-space: nowrap;
}

.read-icon:hover:not(:disabled) {
  color: var(--color-info);
}

.write-icon:hover:not(:disabled) {
  color: var(--accent);
}

.erase-icon:hover:not(:disabled) {
  color: var(--color-danger);
}

.verify-icon:hover:not(:disabled) {
  color: var(--color-warn);
}

.blank-check-icon:hover:not(:disabled) {
  color: var(--color-info);
}

.auto-icon:hover:not(:disabled) {
  color: var(--accent);
}

.auto-gear {
  min-width: 42px;
}

.auto-step-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 10px 0 6px;
}

.auto-order-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 6px;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.auto-order-item.is-dragging {
  opacity: 0.45;
}
.auto-order-item.is-drop-target {
  border-top-color: var(--accent);
  box-shadow: 0 -2px 0 0 var(--accent);
}
.auto-order-move {
  transition: transform 180ms ease;
}
.auto-order-enter-active,
.auto-order-leave-active {
  transition:
    opacity 150ms ease,
    transform 150ms ease;
}
.auto-order-enter-from,
.auto-order-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
.auto-order-handle {
  color: var(--text-muted);
  cursor: grab;
  user-select: none;
  padding: 0 2px;
}
.auto-order-handle:active {
  cursor: grabbing;
}
.auto-order-index {
  width: 16px;
  height: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: var(--accent-subtle);
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 10px;
  flex-shrink: 0;
}
.auto-order-name {
  flex: 1;
  font-size: 12px;
  color: var(--text-primary);
  font-family: var(--font-sans);
}
.auto-order-btn {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
}
.auto-order-btn:hover {
  background: var(--bg-overlay);
  color: var(--text-primary);
}
.auto-order-remove:hover {
  color: var(--color-danger);
}
.auto-order-empty {
  padding: 10px;
  text-align: center;
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-sans);
  border: 1px dashed var(--border);
  border-radius: var(--radius-sm);
}
.auto-step-pool {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 8px 0 4px;
}

.auto-step-summary {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--bg-surface);
  border-radius: var(--radius-sm);
  padding: 6px 8px;
  margin: 4px 0 10px;
  word-break: break-all;
}
</style>
