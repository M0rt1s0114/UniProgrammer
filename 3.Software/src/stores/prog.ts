import { ref, computed, watch } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { t } from '@/i18n'
import { useSettingsStore } from '@/stores/settings'

export interface DetectedChipInfo {
  id: string
  vendor: string
  model: string
  protocol: string
  size: number
  page: number
  sector?: number | null
  block?: number | null
  addr4bit?: number | null
  vcc?: string | null
  spare?: number | null
  pagesPerBlock?: number | null
  isBmm?: boolean | null
}

export interface LogEntry {
  id: number
  time: string
  message: string
  level: 'info' | 'warn' | 'error' | 'success' | 'functionTest'
}

export type OperationStatus = 'idle' | 'running' | 'success' | 'error'
export type DetectStatus = 'idle' | 'detecting' | 'success' | 'programmer_fail' | 'chip_rule_fail'

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

let _logId = 0

export const useProgStore = defineStore('prog', () => {
  const settings = useSettingsStore()

  // 编程器连接
  const status = ref<OperationStatus>('idle')
  const connectedDevice = ref('')
  // CH34X 设置（IMPROG 语义：1.8V 适配器、SPI 模式/时钟）
  const vcc18v = ref(false)
  const spiMode = ref(3)
  const spiFreq = ref(15000)
  // VCC 输出（高危功能）：默认关闭，连接编程器时重置，不持久化
  const vccOutputEnabled = ref(false)
  const vccTargetMv = ref(settings.vccTargetMv)
  const vccFollowChip = ref(false)
  // 芯片信息必须声明在 vccChipMv 之前，否则 computed 初始化时会触发 TDZ 错误
  const chipDetails = ref<DetectedChipInfo | null>(null)
  const VCC_LEVELS = [1200, 1800, 2500, 3300]
  const vccChipMv = computed<number | null>(() => {
    const vcc = chipDetails.value?.vcc
    if (!vcc) return null
    const value = Number.parseFloat(vcc)
    if (Number.isNaN(value)) return null
    const mv = Math.round(value * 1000)
    return VCC_LEVELS.includes(mv) ? mv : null
  })

  watch(vccChipMv, (mv) => {
    if (!vccFollowChip.value) return
    if (mv !== null) {
      vccTargetMv.value = mv
      addLog(t('vcc.followLog').replace('{0}', (mv / 1000).toFixed(1)), 'functionTest')
    } else {
      vccFollowChip.value = false
      addLog(t('vcc.noChipVcc'), 'warn')
    }
  })

  // 芯片检测状态
  const detectStatus = ref<DetectStatus>('idle')
  const chipInfo = ref<string[]>([])

  // 当前操作状态（进度等）
  const isRunning = ref(false)
  const currentOp = ref('')
  const progress = ref(0)
  const progressMessage = ref('')

  // Hex 查看器数据
  const hexData = ref<Uint8Array | null>(null)

  // 文件信息
  const filePath = ref('')
  const fileSize = ref(0)

  // 地址范围（预留）
  const startAddr = ref('0x00000000')
  const lengthVal = ref('')

  // 校验
  const verifyAfterWrite = ref(false)

  // SPI NAND 设置（来自 Setting.set，settings store 为唯一持久化来源）
  const nandReadBadBlockFirst = ref(settings.nandReadBadBlockFirst)
  const nandBadBlockMode = ref<'skip' | 'bypass' | 'ignore'>(settings.nandBadBlockMode)
  const nandProgramMode = ref<'main' | 'oob_auto' | 'main_oob'>(settings.nandProgramMode)
  const nandBatchBurn = ref(settings.batchBurn)
  const nandSaveVoltage = ref(settings.saveVoltage)
  const nandPowerAutoDetect = ref(settings.powerAutoDetect)
  const nandAutoDetectEeprom = ref(settings.autoDetectEeprom)
  const nandProgressEstimate = ref(settings.progressEstimate)
  const nandCheckSoundSwitch = ref(settings.checkSoundSwitch)

  // 设置对话框改动后同步到 prog store，保持现有组件绑定不变
  watch(
    [
      () => settings.batchBurn,
      () => settings.saveVoltage,
      () => settings.powerAutoDetect,
      () => settings.autoDetectEeprom,
      () => settings.progressEstimate,
      () => settings.checkSoundSwitch,
      () => settings.nandReadBadBlockFirst,
      () => settings.nandBadBlockMode,
      () => settings.nandProgramMode,
    ],
    ([
      batchBurn,
      saveVoltage,
      powerAutoDetect,
      autoDetectEeprom,
      progressEstimate,
      checkSoundSwitch,
      readBadBlockFirst,
      badBlockMode,
      programMode,
    ]) => {
      nandBatchBurn.value = batchBurn
      nandSaveVoltage.value = saveVoltage
      nandPowerAutoDetect.value = powerAutoDetect
      nandAutoDetectEeprom.value = autoDetectEeprom
      nandProgressEstimate.value = progressEstimate
      nandCheckSoundSwitch.value = checkSoundSwitch
      nandReadBadBlockFirst.value = readBadBlockFirst
      nandBadBlockMode.value = badBlockMode
      nandProgramMode.value = programMode
    },
  )

  // 电压目标值双向同步：VCC 面板继续用 prog store，Setting.set 用 settings store
  watch(vccTargetMv, (mv) => {
    if (settings.hydrated && mv !== settings.vccTargetMv) {
      settings.vccTargetMv = mv
    }
  })
  watch(
    () => settings.vccTargetMv,
    (mv) => {
      if (mv !== vccTargetMv.value) {
        vccTargetMv.value = mv
      }
    },
  )

  // 日志
  const logs = ref<LogEntry[]>([])

  // 芯片库级联菜单数据
  const chipTypes = ref<string[]>([])
  const chipVendors = ref<string[]>([])
  const chipModels = ref<string[]>([])

  const selectedType = ref('')
  const selectedVendor = ref('')
  const selectedModel = ref('')

  // 芯片检测结果
  const chipDetected = ref(false)
  const detectedChipSize = ref(0)

  // 计算属性
  const isConnected = computed(() => status.value === 'success')
  // 硬件检测只对带 JEDEC ID 的 SPI 芯片有意义；未选类型时也允许检测
  // （连接后直接点“检测”是最常用的路径，与 IMSProg 一致）
  const canDetect = computed(
    () =>
      isConnected.value &&
      (selectedType.value === '' ||
        ['SPI_EC', 'SPI_DATA_45', 'SPI_NAND', 'SPI_NOR', 'SPI_EEPROM', 'SPI_F-RAM'].includes(
          selectedType.value,
        )),
  )
  const canSearch = computed(() => !!selectedModel.value)
  const canOperate = computed(() => isConnected.value && chipDetected.value)
  // SPI NOR 专属操作标记（保留给后续 UI 使用）
  const isSpiNor = computed(() => selectedType.value === 'SPI_NOR')

  // 日志操作
  function addLog(message: string, level: LogEntry['level'] = 'info') {
    const now = new Date().toLocaleTimeString()
    logs.value.push({ id: ++_logId, time: now, message, level })
    if (logs.value.length > 1000) logs.value.shift()
  }

  function clearLogs() {
    logs.value = []
  }

  // 芯片库加载
  async function loadLibAndTypes() {
    try {
      await invoke('load_chip_lib')
      chipTypes.value = await invoke('get_chip_types')
    } catch (e: unknown) {
      addLog(`芯片库初始化失败: ${String(e)}`, 'error')
    }
  }

  // 级联菜单辅助
  async function loadChipVendorsDirect(protocol: string): Promise<string[]> {
    try {
      return await invoke('get_chip_vendors', { protocol })
    } catch (e: unknown) {
      addLog(`加载厂商失败: ${String(e)}`, 'error')
      return []
    }
  }

  async function loadChipModelsDirect(protocol: string, vendor: string): Promise<string[]> {
    try {
      return await invoke('get_chip_models', { protocol, vendor })
    } catch (e: unknown) {
      addLog(`加载型号失败: ${String(e)}`, 'error')
      return []
    }
  }

  async function onTypeChanged() {
    if (selectedType.value) {
      chipVendors.value = await loadChipVendorsDirect(selectedType.value)
    } else {
      chipVendors.value = []
    }
    selectedVendor.value = ''
    chipModels.value = []
    selectedModel.value = ''
    chipDetected.value = false
    detectedChipSize.value = 0
    chipDetails.value = null
  }

  async function onVendorChanged() {
    if (selectedType.value && selectedVendor.value) {
      chipModels.value = await loadChipModelsDirect(selectedType.value, selectedVendor.value)
    } else {
      chipModels.value = []
    }
    selectedModel.value = ''
    chipDetected.value = false
    detectedChipSize.value = 0
    chipDetails.value = null
  }

  // 手动选择型号：I2C/Microwire/EEPROM 无 JEDEC ID，走数据库查询路径
  async function onModelChanged() {
    chipDetected.value = false
    detectedChipSize.value = 0
    chipDetails.value = null
    if (!selectedType.value || !selectedVendor.value || !selectedModel.value) return
    try {
      const info = (await invoke('get_chip_info', {
        protocol: selectedType.value,
        vendor: selectedVendor.value,
        model: selectedModel.value,
      })) as DetectedChipInfo
      chipDetected.value = true
      detectedChipSize.value = info.size
      chipDetails.value = info
      addLog(`已选择: ${info.vendor} ${info.model} (${formatBytes(info.size)})`, 'success')
    } catch (e: unknown) {
      addLog(`加载芯片参数失败: ${String(e)}`, 'error')
    }
  }

  // 连接编程器
  async function initCh34x(kind: 'ch341' | 'ch347' | 'ch347f') {
    status.value = 'running'
    currentOp.value = '初始化 CH34X'
    if (vccOutputEnabled.value) {
      vccOutputEnabled.value = false
      addLog('VCC 输出已重置为关闭', 'warn')
    }
    addLog('正在初始化 CH34X...')
    try {
      const msg = (await invoke('initialize', {
        kind,
        vcc18v: vcc18v.value,
        spiMode: spiMode.value,
        freqKhz: spiFreq.value,
      })) as string
      status.value = 'success'
      connectedDevice.value = msg
      addLog(msg, 'success')
      chipDetected.value = false
      detectedChipSize.value = 0
      chipDetails.value = null
      if (chipTypes.value.length === 0) {
        await loadLibAndTypes()
      }
    } catch (e: unknown) {
      status.value = 'error'
      connectedDevice.value = ''
      addLog(`初始化失败: ${String(e)}`, 'error')
    }
  }

  async function connectSerprog(port: string) {
    status.value = 'running'
    currentOp.value = '连接 Serprog'
    if (vccOutputEnabled.value) {
      vccOutputEnabled.value = false
      addLog('VCC 输出已重置为关闭', 'warn')
    }
    addLog(`正在连接 serprog (${port})...`)
    try {
      const msg = (await invoke('connect_serprog', { port })) as string
      status.value = 'success'
      connectedDevice.value = msg
      addLog(msg, 'success')
      chipDetected.value = false
      detectedChipSize.value = 0
      chipDetails.value = null
      if (chipTypes.value.length === 0) {
        await loadLibAndTypes()
      }
    } catch (e: unknown) {
      status.value = 'error'
      addLog(`serprog 连接失败: ${String(e)}`, 'error')
    }
  }

  // 加载文件（浏览器 <input> 兜底）
  async function loadFile(file: File) {
    try {
      const buffer = await file.arrayBuffer()
      const data = new Uint8Array(buffer)
      hexData.value = data
      filePath.value = file.name
      fileSize.value = file.size
      addLog(`已加载文件: ${file.name} (${formatBytes(file.size)})`, 'success')
    } catch (e: unknown) {
      addLog(`文件加载失败: ${String(e)}`, 'error')
    }
  }

  // 原生文件对话框打开文件（Windows IFileDialog + std::fs）
  async function openFileViaDialog() {
    try {
      const path = await invoke<string | null>('open_file_dialog')
      if (!path) return
      const bytes = await invoke<number[]>('read_file', { path })
      hexData.value = new Uint8Array(bytes)
      filePath.value = path
      fileSize.value = bytes.length
      addLog(`已加载文件: ${path} (${formatBytes(bytes.length)})`, 'success')
    } catch (e: unknown) {
      addLog(`文件加载失败: ${String(e)}`, 'error')
    }
  }

  // 转换芯片库
  async function convertLib() {
    addLog('正在转换芯片库...')
    try {
      const msg = (await invoke('convert_chip_lib')) as string
      addLog(msg, 'success')
    } catch (e: unknown) {
      addLog(`转换失败: ${String(e)}`, 'error')
    }
  }

  return {
    status,
    connectedDevice,
    vcc18v,
    spiMode,
    spiFreq,
    vccOutputEnabled,
    vccTargetMv,
    vccFollowChip,
    vccChipMv,
    detectStatus,
    chipInfo,
    chipDetails,
    isRunning,
    currentOp,
    progress,
    progressMessage,
    hexData,
    filePath,
    fileSize,
    startAddr,
    lengthVal,
    verifyAfterWrite,
    nandReadBadBlockFirst,
    nandBadBlockMode,
    nandProgramMode,
    nandBatchBurn,
    nandSaveVoltage,
    nandPowerAutoDetect,
    nandAutoDetectEeprom,
    nandProgressEstimate,
    nandCheckSoundSwitch,
    logs,
    chipTypes,
    chipVendors,
    chipModels,
    selectedType,
    selectedVendor,
    selectedModel,
    chipDetected,
    detectedChipSize,
    isConnected,
    canDetect,
    canSearch,
    canOperate,
    isSpiNor,
    addLog,
    clearLogs,
    loadLibAndTypes,
    loadChipVendorsDirect,
    loadChipModelsDirect,
    onTypeChanged,
    onVendorChanged,
    onModelChanged,
    initCh34x,
    connectSerprog,
    loadFile,
    openFileViaDialog,
    convertLib,
  }
})
