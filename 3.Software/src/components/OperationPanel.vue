<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { useProgStore, formatBytes } from '@/stores/prog'
import { useSettingsStore } from '@/stores/settings'
import { useSpiNor } from '@/services/spiNor'
import { t } from '@/i18n'
import UiSelect, { type UiOption } from '@/components/UiSelect.vue'

const store = useProgStore()
const settings = useSettingsStore()
const spiNor = useSpiNor()

const programmerType = ref<'ch341' | 'ch347' | 'ch347f' | 'serprog' | 'hidprog'>('ch341')
const serialPort = ref('')

type ExperimentalRequest = {
  title: string
  body: string
  run: () => void | Promise<void>
}
const experimentalRequest = ref<ExperimentalRequest | null>(null)

function requestExperimental(title: string, body: string, run: () => void | Promise<void>) {
  experimentalRequest.value = { title, body, run }
}

function confirmExperimentalAction(labelKey: string, run: () => void | Promise<void>) {
  requestExperimental(t('experimental.title'), `${t(labelKey)}：${t('experimental.body')}`, run)
}

async function confirmExperimental() {
  const request = experimentalRequest.value
  if (!request) return
  experimentalRequest.value = null
  await request.run()
}

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

const chipTypeOptions = computed<UiOption[]>(() =>
  store.chipTypes.map((v) => ({ value: v, label: v })),
)
const chipVendorOptions = computed<UiOption[]>(() =>
  store.chipVendors.map((v) => ({ value: v, label: v })),
)
const chipModelOptions = computed<UiOption[]>(() =>
  store.chipModels.map((v) => ({ value: v, label: v })),
)

// SPI NAND 设置选项
const nandBadBlockOptions = computed<UiOption[]>(() => [
  { value: 'skip', label: t('nand.mode.skip') },
  { value: 'bypass', label: t('nand.mode.bypass') },
  { value: 'ignore', label: t('nand.mode.ignore') },
])
const nandProgramModeOptions = computed<UiOption[]>(() => [
  { value: 'main', label: t('nand.prog.main') },
  { value: 'oob_auto', label: t('nand.prog.oobAuto') },
  { value: 'main_oob', label: t('nand.prog.mainOob') },
])

const nandOtpPage = ref(0)
const nandAdvancedOpen = ref(false)

// VCC 输出（高危功能，默认关闭）
const vccVoltageOptions: UiOption[] = [1200, 1800, 2500, 3300].map((mv) => ({
  value: mv,
  label: `${(mv / 1000).toFixed(1)} V`,
}))
const vccModal = ref(false)
const voltageLabel = computed(() => (store.vccTargetMv / 1000).toFixed(1))
const vccPowerHint = computed(() => t('vcc.modalBodyVoltage').replace('{0}', voltageLabel.value))

function requestVccEnable() {
  if (store.isRunning) return
  vccModal.value = true
}

function confirmVccEnable() {
  vccModal.value = false
  store.vccOutputEnabled = true
  store.addLog(t('vcc.testEnabled').replace('{0}', voltageLabel.value), 'functionTest')
}

function disableVccOutput() {
  store.vccOutputEnabled = false
  store.addLog(t('vcc.testDisabled'), 'functionTest')
}

// 电压调整：断电状态下直接生效，无需数字输入确认；接通电源后由 UI 锁定
function onVccTargetChange(value: string | number) {
  if (store.vccOutputEnabled || store.vccFollowChip || store.isRunning) return
  const target = Number(value)
  if (![1200, 1800, 2500, 3300].includes(target)) return
  const old = voltageLabel.value
  const next = (target / 1000).toFixed(1)
  store.vccTargetMv = target
  store.addLog(t('vcc.testChanged').replace('{0}', old).replace('{1}', next), 'functionTest')
}

function onVccFollowChange() {
  if (store.vccFollowChip && store.vccChipMv !== null) {
    store.vccTargetMv = store.vccChipMv
    store.addLog(
      t('vcc.followLog').replace('{0}', (store.vccChipMv / 1000).toFixed(1)),
      'functionTest',
    )
  }
}

function closeVccModal() {
  vccModal.value = false
}

async function connect() {
  if (
    programmerType.value === 'ch341' ||
    programmerType.value === 'ch347' ||
    programmerType.value === 'ch347f'
  ) {
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
  if (store.status === 'success' && store.nandPowerAutoDetect) {
    store.addLog('上电自动检测已开启，正在自动检测芯片...')
    await spiNor.detectChip()
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
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="2" y="3" width="20" height="14" rx="2" />
          <path d="M8 21h8M12 17v4" />
        </svg>
        {{ t('section.programmer') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.type') }}</label>
        <UiSelect
          v-model="programmerType"
          :options="programmerOptions"
          :disabled="store.status === 'running'"
        />
      </div>

      <div
        v-if="programmerType === 'ch347' || programmerType === 'ch347f'"
        class="field"
        style="margin-top: 6px"
      >
        <label class="field-label">{{ t('label.spiMode') }}</label>
        <UiSelect v-model="store.spiMode" :options="spiModeOptions" />
      </div>

      <div
        v-if="programmerType === 'ch347' || programmerType === 'ch347f'"
        class="field"
        style="margin-top: 6px"
      >
        <label class="field-label">{{ t('label.spiClock') }}</label>
        <UiSelect v-model="store.spiFreq" :options="spiFreqOptions" />
      </div>

      <div v-if="programmerType === 'serprog'" class="field" style="margin-top: 6px">
        <label class="field-label">{{ t('label.serialPort') }}</label>
        <input v-model="serialPort" class="input" :placeholder="t('placeholder.serialPort')" />
      </div>

      <button
        class="btn btn-primary w-full"
        style="margin-top: 8px"
        :disabled="store.status === 'running'"
        @click="connect"
      >
        {{ store.status === 'success' ? t('action.reconnect') : t('action.connect') }}
      </button>

      <!-- 设备名称已移至状态栏，此处移除 -->
      <!-- 转换芯片库按钮隐藏但保留代码 -->
      <button
        class="btn btn-ghost btn-sm"
        style="margin-top: 4px; display: none"
        @click="store.convertLib()"
      >
        转换芯片库 (XML→BIN)
      </button>
    </section>

    <div class="divider" />

    <!-- ── 芯片选择 ── -->
    <section class="panel-section">
      <div class="section-label">
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="2" y="6" width="20" height="12" rx="2" />
          <path d="M8 6V4M12 6V4M16 6V4M8 18v2M12 18v2M16 18v2" />
        </svg>
        {{ t('section.chip') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.type') }}</label>
        <UiSelect
          v-model="store.selectedType"
          :options="chipTypeOptions"
          :placeholder="t('placeholder.selectType')"
          @change="onTypeChange"
        />
      </div>

      <div class="field" style="margin-top: 6px">
        <label class="field-label">{{ t('label.vendor') }}</label>
        <UiSelect
          v-model="store.selectedVendor"
          :options="chipVendorOptions"
          :placeholder="t('placeholder.selectVendor')"
          :disabled="!store.selectedType"
          @change="onVendorChange"
        />
      </div>

      <div class="field" style="margin-top: 6px">
        <label class="field-label">{{ t('label.model') }}</label>
        <UiSelect
          v-model="store.selectedModel"
          :options="chipModelOptions"
          :placeholder="t('placeholder.selectModel')"
          :disabled="!store.selectedVendor"
          @change="onModelChange"
        />
      </div>

      <div style="display: flex; gap: 8px; margin-top: 8px">
        <button
          class="btn btn-secondary"
          style="flex: 1"
          :disabled="!store.canDetect"
          @click="spiNor.detectChip()"
        >
          {{ t('action.detect') }}
        </button>
        <button
          class="btn btn-secondary"
          style="flex: 1"
          :disabled="!store.canSearch"
          @click="store.onModelChanged()"
        >
          {{ t('action.search') }}
        </button>
      </div>
    </section>

    <div v-if="store.selectedType === 'SPI_NAND'" class="divider" />

    <!-- ── NAND 设置（SPI NAND 专属）── -->
    <section v-if="store.selectedType === 'SPI_NAND'" class="panel-section">
      <div class="section-label">{{ t('section.nand') }}</div>

      <label class="toggle-row">
        <input v-model="store.nandReadBadBlockFirst" type="checkbox" class="toggle-check" />
        <span class="toggle-text">{{ t('nand.readBadBlockFirst') }}</span>
      </label>

      <div class="field">
        <label class="field-label">{{ t('nand.badBlockMode') }}</label>
        <UiSelect v-model="store.nandBadBlockMode" :options="nandBadBlockOptions" />
      </div>

      <div class="field" style="margin-top: 6px">
        <label class="field-label">{{ t('nand.programMode') }}</label>
        <UiSelect v-model="store.nandProgramMode" :options="nandProgramModeOptions" />
      </div>

      <button
        class="btn btn-secondary w-full"
        style="margin-top: 6px"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.scanBadBlocks()"
      >
        {{ t('nand.scanBadBlocks') }}
      </button>

      <div class="nand-options-grid">
        <label class="toggle-row">
          <input v-model="store.nandBatchBurn" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.batchBurn') }}</span>
        </label>
        <label class="toggle-row">
          <input v-model="store.nandSaveVoltage" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.saveVoltage') }}</span>
        </label>
        <label class="toggle-row">
          <input v-model="store.nandPowerAutoDetect" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.powerAutoDetect') }}</span>
        </label>
        <label class="toggle-row">
          <input v-model="store.nandAutoDetectEeprom" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.autoDetectEeprom') }}</span>
        </label>
        <label class="toggle-row">
          <input v-model="store.nandProgressEstimate" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.progressEstimate') }}</span>
        </label>
        <label class="toggle-row">
          <input v-model="store.nandCheckSoundSwitch" type="checkbox" class="toggle-check" />
          <span class="toggle-text">{{ t('nand.checkSoundSwitch') }}</span>
        </label>
      </div>

      <button
        class="btn btn-ghost btn-sm w-full adv-toggle"
        :class="{ open: nandAdvancedOpen }"
        style="margin-top: 6px"
        @click="nandAdvancedOpen = !nandAdvancedOpen"
      >
        <span class="adv-toggle-label">
          <span class="adv-warn-dot" />{{ t('nand.advanced') }}
        </span>
        <svg
          class="adv-chevron"
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      <div v-show="nandAdvancedOpen" class="adv-panel">
        <button
          class="btn btn-ghost btn-sm w-full adv-btn"
          :disabled="!store.canOperate || store.isRunning"
          @click="confirmExperimentalAction('nand.readUid', () => spiNor.readNandUid())"
        >
          {{ t('nand.readUid') }}
        </button>
        <button
          class="btn btn-ghost btn-sm w-full adv-btn"
          :disabled="!store.canOperate || store.isRunning"
          @click="confirmExperimentalAction('nand.readParamPage', () => spiNor.readNandParamPage())"
        >
          {{ t('nand.readParamPage') }}
        </button>
        <button
          class="btn btn-ghost btn-sm w-full adv-btn"
          :disabled="!store.canOperate || store.isRunning"
          @click="confirmExperimentalAction('nand.readBbmLut', () => spiNor.readNandBbmLut())"
        >
          {{ t('nand.readBbmLut') }}
        </button>
        <button
          class="btn btn-ghost btn-sm w-full adv-btn"
          :disabled="!store.canOperate || store.isRunning"
          @click="confirmExperimentalAction('nand.eccEnable', () => spiNor.setNandEcc(true))"
        >
          {{ t('nand.eccEnable') }}
        </button>
        <button
          class="btn btn-ghost btn-sm w-full adv-btn"
          :disabled="!store.canOperate || store.isRunning"
          @click="confirmExperimentalAction('nand.eccDisable', () => spiNor.setNandEcc(false))"
        >
          {{ t('nand.eccDisable') }}
        </button>
        <div style="display: flex; gap: 8px; margin-top: 6px">
          <input
            v-model.number="nandOtpPage"
            type="number"
            min="0"
            max="63"
            class="input"
            style="width: 80px"
            :title="t('nand.otpPage')"
          />
          <button
            class="btn btn-ghost btn-sm"
            style="flex: 1"
            :disabled="!store.canOperate || store.isRunning"
            @click="
              confirmExperimentalAction('nand.readOtpPage', () =>
                spiNor.readNandOtpPage(nandOtpPage),
              )
            "
          >
            {{ t('nand.readOtpPage') }}
          </button>
        </div>
      </div>
    </section>

    <div v-if="store.selectedType === 'SPI_DATA_45'" class="divider" />

    <!-- ── 45 芯片模式（DataFlash 专属）── -->
    <section v-if="store.selectedType === 'SPI_DATA_45'" class="panel-section">
      <div class="section-label">{{ t('section.at45') }}</div>

      <div class="at45-btn-col">
        <button
          class="btn btn-ghost btn-sm w-full"
          :disabled="!store.canOperate || store.isRunning"
          @click="
            confirmExperimentalAction('at45.readPageMode', () => spiNor.readAt45PageMode('page'))
          "
        >
          {{ t('at45.readPageMode') }}
        </button>
        <button
          class="btn btn-ghost btn-sm w-full"
          :disabled="!store.canOperate || store.isRunning"
          @click="
            confirmExperimentalAction('at45.readChipMode', () => spiNor.readAt45PageMode('chip'))
          "
        >
          {{ t('at45.readChipMode') }}
        </button>
        <button
          class="btn btn-secondary w-full"
          :disabled="!store.canOperate || store.isRunning"
          @click="
            confirmExperimentalAction('at45.setDataFlashPage', () =>
              spiNor.setAt45PageMode(false, true),
            )
          "
        >
          {{ t('at45.setDataFlashPage') }}
        </button>
        <button
          class="btn btn-secondary w-full"
          :disabled="!store.canOperate || store.isRunning"
          @click="
            confirmExperimentalAction('at45.setBinaryPage', () =>
              spiNor.setAt45PageMode(true, true),
            )
          "
        >
          {{ t('at45.setBinaryPage') }}
        </button>
      </div>
    </section>

    <div class="divider" />

    <!-- ── 芯片信息 ── -->
    <section class="panel-section">
      <div class="section-label">{{ t('section.chipInfo') }}</div>
      <div v-if="store.chipDetected && store.chipDetails" class="chip-info">
        <div class="chip-info-line">
          {{ store.chipDetails.vendor }} {{ store.chipDetails.model }}
        </div>
        <div class="chip-info-line">{{ t('chipInfo.jedec') }} {{ store.chipDetails.id }}</div>
        <div class="chip-info-line">
          {{ t('chipInfo.capacity') }} {{ formatBytes(store.chipDetails.size) }}
        </div>
        <div class="chip-info-line">
          {{ t('chipInfo.page') }} {{ store.chipDetails.page }} B<span
            v-if="store.chipDetails.sector"
          >
            · {{ t('chipInfo.sector') }} {{ store.chipDetails.sector }} B</span
          >
        </div>
        <div v-if="store.chipDetails.block" class="chip-info-line">
          {{ t('chipInfo.block') }} {{ formatBytes(store.chipDetails.block) }}
        </div>
        <div v-if="store.chipDetails.spare" class="chip-info-line">
          {{ t('nand.spare') }} {{ store.chipDetails.spare }} B
        </div>
        <div v-if="store.chipDetails.pagesPerBlock" class="chip-info-line">
          {{ t('nand.pagesPerBlock') }} {{ store.chipDetails.pagesPerBlock }}
        </div>
        <div
          v-if="store.chipDetails.isBmm !== null && store.chipDetails.isBmm !== undefined"
          class="chip-info-line"
        >
          {{ t('nand.isBmm') }}:
          {{ store.chipDetails.isBmm ? t('common.yes') : t('common.no') }}
        </div>
        <div v-if="store.chipDetails.vcc" class="chip-info-line">
          {{ t('chipInfo.vcc') }} {{ store.chipDetails.vcc }} V
        </div>
        <div class="chip-info-line">
          {{ t('chipInfo.addr4') }}
          {{
            store.chipDetails.addr4bit && store.chipDetails.addr4bit & 0x0f
              ? t('common.yes')
              : t('common.no')
          }}
        </div>
      </div>
      <div v-else class="chip-placeholder">{{ t('chipInfo.none') }}</div>
    </section>

    <div v-if="settings.vccControlEnabled" class="divider" />

    <!-- ── 电压调节（设置总开关开启后显示）── -->
    <section v-if="settings.vccControlEnabled" class="panel-section">
      <div class="section-label">
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
        </svg>
        {{ t('section.vcc') }}
      </div>

      <div class="field">
        <label class="field-label">{{ t('label.vccVoltage') }}</label>
        <UiSelect
          :model-value="store.vccTargetMv"
          :options="vccVoltageOptions"
          :disabled="store.vccOutputEnabled || store.vccFollowChip || store.isRunning"
          @change="onVccTargetChange"
        />
      </div>

      <label class="toggle-row">
        <input
          v-model="store.vccFollowChip"
          type="checkbox"
          class="toggle-check"
          :disabled="!store.vccChipMv || store.vccOutputEnabled || store.isRunning"
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
      <div v-if="store.vccOutputEnabled" class="vcc-status">
        {{ t('vcc.statusOn') }} · {{ voltageLabel }} {{ t('vcc.voltageUnit') }}
      </div>
      <div v-if="!store.vccOutputEnabled" class="vcc-hint">{{ t('vcc.offHint') }}</div>
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

  <!-- 实验性功能警告弹窗（黄色等级，低于 VCC 高危红色） -->
  <Transition name="fade">
    <div v-if="experimentalRequest" class="modal-backdrop" @click.self="experimentalRequest = null">
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
        <h3 class="modal-title">{{ experimentalRequest.title }}</h3>
        <p class="modal-body">{{ experimentalRequest.body }}</p>
        <div class="modal-actions">
          <button class="btn btn-secondary" @click="experimentalRequest = null">
            {{ t('action.cancel') }}
          </button>
          <button class="btn btn-warn" @click="confirmExperimental">
            {{ t('experimental.continue') }}
          </button>
        </div>
      </div>
    </div>
  </Transition>

  <!-- VCC 接通电源确认弹窗（高危，无需输入，必须显示目标电压） -->
  <Transition name="fade">
    <div v-if="vccModal" class="modal-backdrop" @click.self="closeVccModal">
      <div class="modal vcc-modal">
        <div class="modal-icon">⚡</div>
        <h3 class="modal-title">{{ t('vcc.modalTitle') }}</h3>
        <p class="modal-body">{{ vccPowerHint }}</p>
        <div class="vcc-voltage-target">{{ voltageLabel }} {{ t('vcc.voltageUnit') }}</div>
        <div class="modal-actions">
          <button class="btn btn-secondary" @click="closeVccModal">
            {{ t('action.cancel') }}
          </button>
          <button class="btn btn-danger" @click="confirmVccEnable">
            {{ t('vcc.connectPower') }}
          </button>
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
  letter-spacing: 0;
  text-transform: uppercase;
  color: var(--text-muted);
  margin-bottom: 2px;
}

.w-full {
  width: 100%;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
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

.toggle-row {
  display: flex;
  align-items: center;
  gap: 7px;
  cursor: pointer;
  padding: 2px 0 2px 4px;
}

.nand-options-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px 10px;
  margin-top: 6px;
}

/* 45 模式按钮改为纵向全宽排列，避免中文标签超出 230px 侧栏 */
.at45-btn-col {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.adv-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px !important;
  color: var(--color-warn);
}
.adv-toggle:hover {
  color: var(--color-warn);
}
.adv-toggle-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
}
.adv-warn-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-warn);
  box-shadow: 0 0 6px var(--warn-border);
}
.adv-chevron {
  transition: transform 150ms ease;
}
.adv-toggle.open .adv-chevron {
  transform: rotate(180deg);
}
.adv-panel {
  margin-top: 4px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px;
  border: 1px solid var(--warn-border);
  border-radius: var(--radius-md);
  background: var(--warn-soft);
}
.adv-btn {
  text-align: left;
  padding-left: 10px !important;
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
  border: 1px solid var(--info-border);
}

.running-label {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--color-info);
  text-transform: capitalize;
}

.vcc-box {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 8px;
  background: var(--bg-surface);
}
.vcc-power-btn {
  margin-top: 6px;
}
.vcc-box.vcc-active {
  border-color: var(--danger-border);
  background: var(--danger-soft);
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
.vcc-toggle:hover {
  border-color: var(--border-focus);
}
.vcc-toggle.is-on {
  border-color: var(--danger-border);
  background: var(--danger-soft);
  color: var(--color-danger);
  font-weight: 600;
}

.vcc-status {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--color-danger);
  font-weight: 600;
}

.vcc-hint {
  margin-top: 4px;
  font-size: 10px;
  color: var(--text-muted);
}

.vcc-voltage-target {
  margin: 2px auto 0;
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 700;
  color: var(--color-danger);
  border: 1px solid var(--danger-border);
  background: var(--danger-soft);
  border-radius: var(--radius-md);
  padding: 6px 16px;
}

.vcc-modal .modal-icon {
  color: var(--color-danger);
}
.vcc-modal .modal-title {
  color: var(--color-danger);
}
</style>
