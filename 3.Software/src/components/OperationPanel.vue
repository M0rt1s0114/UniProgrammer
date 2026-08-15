<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { useProgStore, formatBytes } from '@/stores/prog'
import { useSpiNor } from '@/services/spiNor'
import { t } from '@/i18n'
import UiSelect, { type UiOption } from '@/components/UiSelect.vue'

const store = useProgStore()
const spiNor = useSpiNor()

const programmerType = ref<'ch341' | 'ch347' | 'ch347f' | 'serprog' | 'hidprog'>('ch341')
const serialPort = ref('')

const programmerOptions: UiOption[] = [
  { value: 'ch341', label: 'CH341A' },
  { value: 'ch347', label: 'CH347T' },
  { value: 'ch347f', label: 'CH347F' },
  { value: 'serprog', label: t('option.serprog') },
  { value: 'hidprog', label: t('option.hidprog') },
]

const spiModeOptions: UiOption[] = [
  { value: 0, label: 'Mode 0 (CPOL=0, CPHA=0)' },
  { value: 1, label: 'Mode 1 (CPOL=0, CPHA=1)' },
  { value: 2, label: 'Mode 2 (CPOL=1, CPHA=0)' },
  { value: 3, label: 'Mode 3 (CPOL=1, CPHA=1)' },
]

const spiFreqOptions: UiOption[] = [
  { value: 60000, label: '60 MHz' },
  { value: 30000, label: '30 MHz' },
  { value: 15000, label: '15 MHz' },
  { value: 7500, label: '7.5 MHz' },
  { value: 3750, label: '3.75 MHz' },
  { value: 1875, label: '1.875 MHz' },
  { value: 937, label: '937.5 KHz' },
  { value: 469, label: '468.75 KHz' },
]

const chipTypeOptions = computed<UiOption[]>(() => store.chipTypes.map(v => ({ value: v, label: v })))
const chipVendorOptions = computed<UiOption[]>(() => store.chipVendors.map(v => ({ value: v, label: v })))
const chipModelOptions = computed<UiOption[]>(() => store.chipModels.map(v => ({ value: v, label: v })))

// VCC 输出（高危功能，默认关闭）
const vccVoltageOptions: UiOption[] = [1200, 1800, 2500, 3300].map(mv => ({
  value: mv,
  label: `${(mv / 1000).toFixed(1)} V`,
}))
const vccModal = ref<'enable' | 'change' | null>(null)
const vccConfirmText = ref('')
const pendingVccTarget = ref<number | null>(null)
const voltageLabel = computed(() => (store.vccTargetMv / 1000).toFixed(1))
const vccEnableHint = computed(() =>
  t('vcc.typeHint').replace('{0}', t('vcc.enablePhrase')),
)
const vccChangeHint = computed(() => {
  if (pendingVccTarget.value === null) return ''
  return t('vcc.changePhraseHint').replace('{0}', (pendingVccTarget.value / 1000).toFixed(1))
})

function requestVccEnable() {
  if (store.isRunning) return
  vccConfirmText.value = ''
  vccModal.value = 'enable'
}

function confirmVccEnable() {
  if (vccConfirmText.value.trim() !== t('vcc.enablePhrase')) {
    store.addLog(t('vcc.wrongPhrase'), 'warn')
    return
  }
  vccModal.value = null
  store.vccOutputEnabled = true
  store.addLog(t('vcc.testEnabled').replace('{0}', voltageLabel.value), 'functionTest')
}

function disableVccOutput() {
  store.vccOutputEnabled = false
  store.addLog(t('vcc.testDisabled'), 'functionTest')
}

function requestVccTarget(mv: number) {
  if (!store.vccOutputEnabled || store.vccFollowChip || store.isRunning) return
  pendingVccTarget.value = mv
  vccConfirmText.value = ''
  vccModal.value = 'change'
}

function onVccFollowChange() {
  if (store.vccFollowChip && store.vccChipMv !== null) {
    store.vccTargetMv = store.vccChipMv
    store.addLog(t('vcc.followLog').replace('{0}', (store.vccChipMv / 1000).toFixed(1)), 'functionTest')
  }
}

function confirmVccTarget() {
  const target = pendingVccTarget.value
  if (target === null) return
  const expected = (target / 1000).toFixed(1)
  if (vccConfirmText.value.trim() !== expected) {
    store.addLog(t('vcc.wrongPhrase'), 'warn')
    return
  }
  const old = voltageLabel.value
  vccModal.value = null
  store.vccTargetMv = target
  store.addLog(t('vcc.testChanged').replace('{0}', old).replace('{1}', expected), 'functionTest')
}

function closeVccModal() {
  vccModal.value = null
}

const fileInput = ref<HTMLInputElement | null>(null)

function openFileDialog() {
  // 原生对话框（Windows IFileDialog）；隐藏 <input> 仅作兜底保留
  store.openFileViaDialog()
}

async function onFileSelected(event: Event) {
  const input = event.target as HTMLInputElement
  if (input.files && input.files.length > 0) {
    const file = input.files[0]
    await store.loadFile(file)
  }
  if (input) input.value = ''
}

const showEraseConfirm = ref(false)

function requestErase() {
  showEraseConfirm.value = true
}

async function confirmErase() {
  showEraseConfirm.value = false
  await spiNor.eraseChip()
}

async function connect() {
  if (programmerType.value === 'ch341' || programmerType.value === 'ch347' || programmerType.value === 'ch347f') {
    await store.initCh34x(programmerType.value)
  } else if (programmerType.value === 'hidprog') {
    // 预留给自有 HIDProg 编程器项目，暂不实现任何功能
    store.addLog('HIDProg 为预留选项，暂未实现', 'warn')
    return
  } else {
    const port = serialPort.value.trim()
    if (!port) {
      store.addLog('请输入串口号', 'warn')
      return
    }
    await store.connectSerprog(port)
  }
}

async function onTypeChange() {
  await store.onTypeChanged()
}

async function onVendorChange() {
  await store.onVendorChanged()
}

async function onModelChange() {
  await store.onModelChanged()
}

onMounted(async () => {
  await store.loadLibAndTypes()
})
</script>

<template>
  <div class="op-panel">

    <!-- ── 编程器连接 ── -->
    <section class="panel-section">
      <div class="section-label">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="2" y="3" width="20" height="14" rx="2"/>
          <path d="M8 21h8M12 17v4"/>
        </svg>
        {{ t('section.programmer') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.type') }}</label>
        <UiSelect v-model="programmerType" :options="programmerOptions" :disabled="store.status === 'running'" />
      </div>

      <div v-if="programmerType === 'ch341' || programmerType === 'ch347'" class="field" style="margin-top: 6px;">
        <label class="toggle-row" style="cursor: pointer;">
          <input v-model="store.vcc18v" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('label.vcc18Adapter') }}</span>
        </label>
      </div>

      <div v-if="programmerType === 'ch347' || programmerType === 'ch347f'" class="field" style="margin-top: 6px;">
        <label class="field-label">{{ t('label.spiMode') }}</label>
        <UiSelect v-model="store.spiMode" :options="spiModeOptions" />
      </div>

      <div v-if="programmerType === 'ch347' || programmerType === 'ch347f'" class="field" style="margin-top: 6px;">
        <label class="field-label">{{ t('label.spiClock') }}</label>
        <UiSelect v-model="store.spiFreq" :options="spiFreqOptions" />
      </div>

      <div v-if="programmerType === 'serprog'" class="field" style="margin-top: 6px;">
        <label class="field-label">{{ t('label.serialPort') }}</label>
        <input v-model="serialPort" class="input" :placeholder="t('placeholder.serialPort')" />
      </div>

      <button class="btn btn-primary w-full" style="margin-top: 8px;" @click="connect" :disabled="store.status === 'running'">
        {{ store.status === 'success' ? t('action.reconnect') : t('action.connect') }}
      </button>

      <!-- 设备名称已移至状态栏，此处移除 -->
      <!-- 转换芯片库按钮隐藏但保留代码 -->
      <button class="btn btn-ghost btn-sm" style="margin-top: 4px; display: none;" @click="store.convertLib()">转换芯片库 (XML→BIN)</button>
    </section>

    <div class="divider" />

    <!-- ── 文件 ── -->
    <section class="panel-section">
      <div class="section-label">{{ t('section.file') }}</div>
      <input ref="fileInput" type="file" style="display: none" @change="onFileSelected" />
      <button class="btn btn-secondary w-full" @click="openFileDialog">
        {{ t('action.openFile') }}
      </button>
      <div style="display: flex; gap: 8px;">
        <button class="btn btn-ghost btn-sm" style="flex: 1;" :disabled="!store.hexData" @click="spiNor.saveFileNative('bin')">
          {{ t('action.saveBin') }}
        </button>
        <button class="btn btn-ghost btn-sm" style="flex: 1;" :disabled="!store.hexData" @click="spiNor.saveFileNative('hex')">
          {{ t('action.saveHex') }}
        </button>
      </div>
      <div v-if="store.filePath" class="file-badge">{{ store.filePath }}</div>
    </section>

    <div class="divider" />

    <!-- ── 芯片选择 ── -->
    <section class="panel-section">
      <div class="section-label">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="2" y="6" width="20" height="12" rx="2"/>
          <path d="M8 6V4M12 6V4M16 6V4M8 18v2M12 18v2M16 18v2"/>
        </svg>
        {{ t('section.chip') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.type') }}</label>
        <UiSelect v-model="store.selectedType" :options="chipTypeOptions" :placeholder="t('placeholder.selectType')" @change="onTypeChange" />
      </div>

      <div class="field" style="margin-top: 6px;">
        <label class="field-label">{{ t('label.vendor') }}</label>
        <UiSelect v-model="store.selectedVendor" :options="chipVendorOptions" :placeholder="t('placeholder.selectVendor')" :disabled="!store.selectedType" @change="onVendorChange" />
      </div>

      <div class="field" style="margin-top: 6px;">
        <label class="field-label">{{ t('label.model') }}</label>
        <UiSelect v-model="store.selectedModel" :options="chipModelOptions" :placeholder="t('placeholder.selectModel')" :disabled="!store.selectedVendor" @change="onModelChange" />
      </div>

      <div style="display: flex; gap: 8px; margin-top: 8px;">
        <button class="btn btn-secondary" style="flex: 1;" :disabled="!store.canDetect" @click="spiNor.detectChip()">
          {{ t('action.detect') }}
        </button>
        <button class="btn btn-secondary" style="flex: 1;" :disabled="!store.canSearch" @click="store.onModelChanged()">
          {{ t('action.search') }}
        </button>
      </div>
    </section>

    <div class="divider" />

    <!-- ── 芯片信息 ── -->
    <section class="panel-section">
      <div class="section-label">{{ t('section.chipInfo') }}</div>
      <div v-if="store.chipDetected && store.chipDetails" class="chip-info">
        <div class="chip-info-line">{{ store.chipDetails.vendor }} {{ store.chipDetails.model }}</div>
        <div class="chip-info-line">{{ t('chipInfo.jedec') }} {{ store.chipDetails.id }}</div>
        <div class="chip-info-line">{{ t('chipInfo.capacity') }} {{ formatBytes(store.chipDetails.size) }}</div>
        <div class="chip-info-line">{{ t('chipInfo.page') }} {{ store.chipDetails.page }} B<span v-if="store.chipDetails.sector"> · {{ t('chipInfo.sector') }} {{ store.chipDetails.sector }} B</span></div>
        <div class="chip-info-line" v-if="store.chipDetails.block">{{ t('chipInfo.block') }} {{ formatBytes(store.chipDetails.block) }}</div>
        <div class="chip-info-line" v-if="store.chipDetails.vcc">{{ t('chipInfo.vcc') }} {{ store.chipDetails.vcc }} V</div>
        <div class="chip-info-line">{{ t('chipInfo.addr4') }} {{ store.chipDetails.addr4bit && (store.chipDetails.addr4bit & 0x0f) ? t('common.yes') : t('common.no') }}</div>
      </div>
      <div v-else class="chip-placeholder">{{ t('chipInfo.none') }}</div>
    </section>

    <div class="divider" />

    <!-- ── 电压调节 ── -->
    <section class="panel-section">
      <div class="section-label">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>
        </svg>
        {{ t('section.vcc') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.vccVoltage') }}</label>
        <UiSelect
          :model-value="store.vccTargetMv"
          :options="vccVoltageOptions"
          :disabled="!store.vccOutputEnabled || store.vccFollowChip || store.isRunning"
          @change="requestVccTarget"
        />
      </div>

      <label class="toggle-row">
        <input
          v-model="store.vccFollowChip"
          type="checkbox"
          class="toggle-check"
          :disabled="!store.vccChipMv || store.isRunning"
          @change="onVccFollowChange"
        />
        <span class="toggle-text">{{ t('vcc.followChip') }}</span>
      </label>
      <div class="vcc-hint">{{ store.vccChipMv ? t('vcc.followHint') : t('vcc.noChipVcc') }}</div>

      <button
        class="btn w-full vcc-power-btn"
        :class="{ 'btn-secondary': !store.vccOutputEnabled, 'btn-danger': store.vccOutputEnabled }"
        :disabled="store.isRunning"
        @click="store.vccOutputEnabled ? disableVccOutput() : requestVccEnable()"
      >
        {{ store.vccOutputEnabled ? t('vcc.disconnectPower') : t('vcc.connectPower') }}
      </button>
      <div v-if="store.vccOutputEnabled" class="vcc-status">{{ t('vcc.statusOn') }} · {{ voltageLabel }} {{ t('vcc.voltageUnit') }}</div>
      <div v-if="!store.vccOutputEnabled" class="vcc-hint">{{ t('vcc.offHint') }}</div>
    </section>

    <div class="divider" />

    <!-- ── 操作 ── -->
    <section class="panel-section">
      <div class="section-label">{{ t('section.operations') }}</div>

      <button class="btn btn-secondary w-full op-btn" :disabled="!store.canOperate" @click="spiNor.readChip()">
        <span class="op-label">{{ t('action.read') }}</span>
      </button>

      <button class="btn btn-secondary w-full op-btn" :disabled="!store.canOperate" @click="spiNor.writeChip()">
        <span class="op-label">{{ t('action.write') }}</span>
      </button>

      <button class="btn btn-danger w-full op-btn" :disabled="!store.canOperate" @click="requestErase()">
        <span class="op-label">{{ t('action.erase') }}</span>
      </button>

      <button class="btn btn-secondary w-full op-btn" :disabled="!store.canOperate" @click="spiNor.verifyChip()">
        <span class="op-label">{{ t('action.verify') }}</span>
      </button>
    </section>

    <!-- 运行状态条 -->
    <Transition name="slide-up">
      <div v-if="store.isRunning" class="running-bar">
        <div class="progress-track">
          <div class="progress-fill" :style="{ width: store.progress + '%' }" />
        </div>
        <div class="running-label">{{ store.currentOp }} — {{ Math.round(store.progress) }}%</div>
      </div>
    </Transition>

  </div>

  <!-- VCC 输出确认弹窗（高危） -->
  <Transition name="fade">
    <div v-if="vccModal" class="modal-backdrop" @click.self="closeVccModal">
      <div class="modal vcc-modal">
        <div class="modal-icon">⚡</div>
        <h3 class="modal-title">
          {{ vccModal === 'enable' ? t('vcc.modalTitle') : t('vcc.changeTitle') }}
        </h3>
        <p class="modal-body">
          {{ vccModal === 'enable' ? t('vcc.modalBody') : t('vcc.changeBody') }}
        </p>
        <p v-if="vccModal === 'enable'" class="vcc-confirm-hint">{{ vccEnableHint }}</p>
        <p v-else class="vcc-confirm-hint">{{ vccChangeHint }}</p>
        <input
          v-model="vccConfirmText"
          class="input vcc-confirm-input"
          :placeholder="vccModal === 'enable' ? t('vcc.enablePhrase') : (pendingVccTarget ? (pendingVccTarget / 1000).toFixed(1) : '')"
          @keydown.enter="vccModal === 'enable' ? confirmVccEnable() : confirmVccTarget()"
        />
        <div class="modal-actions">
          <button class="btn btn-secondary" @click="closeVccModal">{{ t('action.cancel') }}</button>
          <button class="btn btn-danger" @click="vccModal === 'enable' ? confirmVccEnable() : confirmVccTarget()">
            {{ vccModal === 'enable' ? t('vcc.connectPower') : t('vcc.apply') }}
          </button>
        </div>
      </div>
    </div>
  </Transition>

  <!-- 擦除确认弹窗 -->
  <Transition name="fade">
    <div v-if="showEraseConfirm" class="modal-backdrop" @click.self="showEraseConfirm = false">
      <div class="modal">
        <div class="modal-icon">⚠️</div>
        <h3 class="modal-title">{{ t('modal.eraseTitle') }}</h3>
        <p class="modal-body">{{ t('modal.eraseBody') }}</p>
        <div class="modal-actions">
          <button class="btn btn-secondary" @click="showEraseConfirm = false">{{ t('action.cancel') }}</button>
          <button class="btn btn-danger" @click="confirmErase">{{ t('action.confirmErase') }}</button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.op-panel {
  display: flex;
  flex-direction: column;
  gap: 0;
  overflow-y: auto;
  padding-bottom: 8px;
}

.panel-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
}

.section-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-muted);
  margin-bottom: 2px;
}

.w-full { width: 100%; }

.field { display: flex; flex-direction: column; gap: 4px; }
.field-label {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: var(--font-sans);
}

.chip-info {
  background: var(--bg-base);
  border: 1px solid var(--border-accent);
  border-radius: var(--radius-md);
  padding: 8px 10px;
}

.chip-info-line {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  line-height: 1.7;
}

.chip-placeholder {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-sans);
  padding: 4px 0;
}

.chip-placeholder--error {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--color-danger);
}

.file-badge {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--accent-subtle);
  border: 1px solid var(--border-accent);
  border-radius: var(--radius-sm);
  padding: 4px 8px;
  color: var(--accent);
}

.file-badge-name {
  font-family: var(--font-mono);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.op-btn {
  display: grid !important;
  grid-template-columns: 20px 1fr auto;
  align-items: center;
  gap: 8px;
  text-align: left;
  padding: 8px 12px !important;
}

.op-icon { display: flex; align-items: center; }
.op-label { font-weight: 500; font-size: 13px; }
.op-desc  { font-size: 10px; color: var(--text-muted); font-family: var(--font-mono); }

.read-icon   { color: var(--color-info); }
.write-icon  { color: var(--accent); }
.erase-icon  { color: var(--color-danger); }
.verify-icon { color: var(--color-warn); }

.toggle-row {
  display: flex;
  align-items: center;
  gap: 7px;
  cursor: pointer;
  padding: 2px 0 2px 4px;
}

.toggle-check {
  accent-color: var(--accent);
  cursor: pointer;
  width: 14px;
  height: 14px;
}

.toggle-text {
  font-size: 12px;
  color: var(--text-secondary);
  font-family: var(--font-sans);
}

.running-bar {
  margin: 0 12px;
  padding: 10px;
  background: var(--bg-elevated);
  border-radius: var(--radius-md);
  border: 1px solid rgba(74, 158, 255, 0.2);
}

.running-label {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: #4a9eff;
  text-transform: capitalize;
}

.vcc-box {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 8px;
  background: var(--bg-surface);
}
.vcc-power-btn { margin-top: 6px; }
.vcc-box.vcc-active {
  border-color: rgba(240, 80, 80, 0.65);
  background: rgba(240, 80, 80, 0.08);
}

.vcc-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.vcc-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
}

.vcc-toggle {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 10px;
  cursor: pointer;
}
.vcc-toggle:hover { border-color: var(--border-focus); }
.vcc-toggle.is-on {
  border-color: rgba(240, 80, 80, 0.8);
  background: rgba(240, 80, 80, 0.15);
  color: #f05050;
  font-weight: 600;
}

.vcc-status {
  font-family: var(--font-mono);
  font-size: 11px;
  color: #f05050;
  font-weight: 600;
}

.vcc-hint {
  margin-top: 4px;
  font-size: 10px;
  color: var(--text-muted);
}

.vcc-confirm-hint {
  font-size: 11px;
  color: var(--color-danger);
}

.vcc-confirm-input {
  text-align: center;
}

.vcc-modal .modal-icon { color: var(--color-warn); }

.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
  backdrop-filter: blur(4px);
}

.modal {
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 28px 24px 20px;
  max-width: 340px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
  text-align: center;
}

.modal-icon { color: var(--color-danger); margin: 0 auto; }
.modal-title { font-size: 16px; font-weight: 600; }

.modal-body {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
  font-family: var(--font-sans);
  white-space: pre-line;
}

.modal-body strong { color: var(--text-primary); }

.modal-actions {
  display: flex;
  gap: 8px;
  justify-content: center;
  margin-top: 8px;
}

.modal-actions .btn { min-width: 96px; }
</style>