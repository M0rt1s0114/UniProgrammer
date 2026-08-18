import { call, onEvent, onProgress, type UnlistenFn } from '@/services/ipc'
import { useProgStore, type DetectedChipInfo, formatBytes } from '@/stores/prog'
import { useSettingsStore } from '@/stores/settings'

export function useSpiNor() {
  const store = useProgStore()
  const settings = useSettingsStore()
  let autoStepFailed = false

  // 预填充上限：NAND 等大容量芯片不预分配 FF，避免 WebView 内存爆炸
  const MAX_PREFILL = 8 * 1024 * 1024

  function playVerifySound() {
    try {
      const ctx = new AudioContext()
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.type = 'sine'
      osc.frequency.value = 880
      gain.gain.setValueAtTime(0.08, ctx.currentTime)
      gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.12)
      osc.connect(gain)
      gain.connect(ctx.destination)
      osc.start()
      osc.stop(ctx.currentTime + 0.13)
    } catch {
      // 音频不可用时静默忽略
    }
  }

  // 通用设置“进度条估算(速度快)”：按已处理字节和耗时线性估算剩余时间。
  // 只追加到进度文本，不改变真实进度事件。
  function formatEstimatedRemaining(startMs: number, done: number, total: number): string {
    if (done <= 0 || total <= 0) return '--'
    const elapsed = Math.max(0, Date.now() - startMs) / 1000
    const remaining = Math.max(0, (elapsed / done) * (total - done))
    if (remaining < 1) return '<1s'
    if (remaining < 60) return `${Math.ceil(remaining)}s`
    const minutes = Math.floor(remaining / 60)
    const seconds = Math.round(remaining % 60)
    return `${minutes}m${seconds.toString().padStart(2, '0')}s`
  }

  function withEstimate(base: string, startMs: number, done: number, total: number): string {
    if (!store.nandProgressEstimate) return base
    return `${base} · 预计 ${formatEstimatedRemaining(startMs, done, total)}`
  }

  function fillHexWithFF(size: number) {
    if (size > MAX_PREFILL) {
      store.hexData = null
      store.addLog(`芯片容量 ${formatBytes(size)} 较大，未预填充缓冲区；可加载文件后写入`, 'warn')
      return
    }
    const buf = new Uint8Array(size)
    buf.fill(0xff)
    store.hexData = buf
  }

  // ── 通用：订阅一个进度 event，返回取消订阅函数 ──────────────────────────────
  async function listenProgress(
    eventName: string,
    handleProgress: (done: number, total: number) => void,
  ): Promise<UnlistenFn> {
    return onProgress<{ done: number; total: number }>(eventName, (payload) => {
      handleProgress(payload.done, payload.total)
    })
  }

  // ── 检测芯片 ────────────────────────────────────────────────────────────────
  async function detectChip() {
    store.isRunning = true
    store.detectStatus = 'detecting'
    store.currentOp = '检测芯片'
    store.addLog('正在检测 SPI Flash...')
    try {
      const result = (await call('detect_chip')) as {
        text: string
        info: DetectedChipInfo | null
      }
      store.detectStatus = 'success'
      store.chipInfo = result.text.split('\n')
      store.addLog(result.text, 'success')
      if (result.info) {
        store.selectedType = result.info.protocol
        store.chipVendors = await store.loadChipVendorsDirect(result.info.protocol)
        store.selectedVendor = result.info.vendor
        store.chipModels = await store.loadChipModelsDirect(
          result.info.protocol,
          result.info.vendor,
        )
        store.selectedModel = result.info.model
        store.detectedChipSize = result.info.size
        store.chipDetails = result.info
        store.chipDetected = true
        fillHexWithFF(result.info.size)
      } else {
        store.chipDetected = false
        store.detectedChipSize = 0
        store.chipDetails = null
      }
    } catch (e: unknown) {
      store.detectStatus = 'chip_rule_fail'
      store.chipInfo = []
      store.addLog(`检测失败: ${String(e)}`, 'error')
      store.chipDetected = false
      store.detectedChipSize = 0
      store.chipDetails = null
      if (typeof e === 'string' && e.includes('传输失败')) {
        store.status = 'error'
        store.connectedDevice = ''
      }
    } finally {
      store.isRunning = false
    }
  }

  // ── 自动检测芯片（n 次，间隔可设，检测到立即停止）────────────────────────
  let chipAutoDetectRunId = 0

  function cancelChipAutoDetect() {
    chipAutoDetectRunId += 1
  }

  async function autoDetectChip() {
    if (store.status !== 'success') {
      store.addLog('请先连接编程器', 'warn')
      return
    }
    const runId = ++chipAutoDetectRunId
    const total = Math.max(1, Math.round(settings.chipAutoDetectCount))
    const intervalMs = Math.max(500, Math.round(settings.chipAutoDetectIntervalSec * 1000))
    for (let attempt = 1; attempt <= total; attempt += 1) {
      if (runId !== chipAutoDetectRunId || !settings.chipAutoDetectEnabled) return
      store.addLog(`自动检测芯片 (${attempt}/${total})`)
      await detectChip()
      if (runId !== chipAutoDetectRunId || !settings.chipAutoDetectEnabled) return
      if (store.chipDetected) {
        store.addLog('自动检测到芯片，停止检测', 'success')
        return
      }
      if (attempt < total) {
        await new Promise((resolve) => setTimeout(resolve, intervalMs))
      }
    }
    if (runId === chipAutoDetectRunId) {
      store.addLog('自动检测结束，未找到芯片', 'warn')
    }
  }

  // ── NAND 高级只读命令（实验性，真机验证前谨慎对待）────────────────────────
  async function runNandRawRead(
    command: 'read_nand_uid' | 'read_nand_param_page' | 'read_nand_bbm_lut',
    label: string,
  ) {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog(`${label} 仅支持已连接的 SPI NAND 芯片`, 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = label
    store.addLog(`${label}：实验性功能，开始执行`, 'functionTest')
    try {
      const result = (await call(command)) as { length: number; hex: string }
      store.addLog(`${label} 完成（${result.length} 字节，实验性命令，需真机验证）`, 'functionTest')
      store.addLog(result.hex, 'info')
    } catch (e: unknown) {
      store.addLog(`${label} 失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  function readNandUid() {
    return runNandRawRead('read_nand_uid', '读取 NAND UID')
  }

  function readNandParamPage() {
    return runNandRawRead('read_nand_param_page', '读取 NAND 参数页')
  }

  async function readNandBbmLut() {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('读取 BBM 映射表仅支持已连接的 SPI NAND 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = '读取 NAND BBM 映射表'
    store.addLog('读取 NAND BBM 映射表：实验性功能，开始执行', 'functionTest')
    try {
      const result = (await call('read_nand_bbm_lut')) as {
        length: number
        hex: string
        entries: { index: number; lba: number; pba: number; free: boolean; valid: boolean }[]
      }
      store.addLog(`BBM 映射表读取完成（${result.length} 字节，实验性）`, 'functionTest')
      for (const e of result.entries) {
        const status = e.free ? '空闲' : e.valid ? '有效替换' : '失效/保留'
        store.addLog(
          `第${e.index + 1}组 | LBA=0x${(e.lba & 0x3fff).toString(16)} | PBA=0x${e.pba.toString(16)} | ${status}`,
          'info',
        )
      }
      store.addLog(result.hex, 'info')
    } catch (e: unknown) {
      store.addLog(`BBM 映射表读取失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  async function readNandOtpPage(page: number) {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('OTP 读取仅支持已连接的 SPI NAND 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = `读取 NAND OTP 页 ${page}`
    store.addLog(`读取 NAND OTP 页 ${page}：实验性功能，开始执行`, 'functionTest')
    try {
      const result = (await call('read_nand_otp_page', { page })) as {
        length: number
        hex: string
      }
      store.addLog(`NAND OTP 页 ${page} 读取完成（${result.length} 字节，实验性）`, 'functionTest')
      store.addLog(result.hex, 'info')
    } catch (e: unknown) {
      store.addLog(`NAND OTP 页 ${page} 读取失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  async function setNandEcc(enable: boolean) {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('ECC 设置仅支持已连接的 SPI NAND 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = enable ? '开启硬件 ECC' : '关闭硬件 ECC'
    store.addLog(`${store.currentOp}：实验性功能，开始执行`, 'functionTest')
    try {
      const enabled = (await call('set_nand_ecc', { enable })) as boolean
      store.addLog(
        `芯片内置 ECC 已${enabled ? '开启' : '关闭'}（实验性，需真机验证）`,
        'functionTest',
      )
    } catch (e: unknown) {
      store.addLog(`ECC 设置失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 45 系列 DataFlash 页面模式（实验性，需真机验证）─────────────────────────
  async function readAt45PageMode(kind: 'page' | 'chip') {
    if (store.selectedType !== 'SPI_DATA_45' || !store.canOperate) {
      store.addLog('45 页面模式读取仅支持已连接的 DataFlash 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = kind === 'page' ? '读45页面模式' : '读45芯片模式'
    store.addLog(`${store.currentOp}：实验性功能，开始执行`, 'functionTest')
    try {
      const result = (await call('read_at45_page_mode')) as { raw: number; binaryPage: boolean }
      store.addLog(
        `45 状态寄存器原始值：0x${result.raw.toString(16).padStart(2, '0')}；当前模式：${
          result.binaryPage ? '二进制页面（2 的幂）' : '标准 DataFlash 页面'
        }（实验性，需真机验证）`,
        'functionTest',
      )
    } catch (e: unknown) {
      store.addLog(`45 页面模式读取失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  async function setAt45PageMode(binary: boolean, skipConfirm = false) {
    if (store.selectedType !== 'SPI_DATA_45' || !store.canOperate) {
      store.addLog('45 页面模式设置仅支持已连接的 DataFlash 芯片', 'warn')
      return
    }
    if (!skipConfirm) {
      const confirmed = window.confirm(
        binary ? t('at45.confirmBinary') : t('at45.confirmDataFlash'),
      )
      if (!confirmed) return
    }
    store.isRunning = true
    store.currentOp = binary ? '切换为二进制页面模式' : '切换为 DataFlash 页面模式'
    store.addLog(`${store.currentOp}：实验性功能，开始执行`, 'functionTest')
    try {
      const result = (await call('set_at45_page_mode', { binary })) as {
        raw: number
        binaryPage: boolean
      }
      store.addLog(
        `45 芯片页面模式切换完成，状态寄存器原始值：0x${result.raw
          .toString(16)
          .padStart(2, '0')}（实验性，需真机验证）`,
        'functionTest',
      )
    } catch (e: unknown) {
      store.addLog(`45 页面模式切换失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 坏块扫描（SPI NAND）────────────────────────────────────────────────────
  // ── SPI NOR 写保护 ─────────────────────────────────────────────────────────
  async function checkNorWriteProtect() {
    if (store.selectedType !== 'SPI_NOR' || !store.canOperate) {
      store.addLog('写保护检查仅支持已连接的 SPI NOR 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = '检查 NOR 写保护'
    try {
      const status = (await call('nor_wp_status')) as {
        sr1: number
        sr2: number
        sr3: number
        bpBits: number
        writeProtected: boolean
      }
      store.addLog(
        `NOR 写保护状态：SR1=0x${status.sr1.toString(16).padStart(2, '0')} SR2=0x${status.sr2
          .toString(16)
          .padStart(2, '0')} SR3=0x${status.sr3
          .toString(16)
          .padStart(
            2,
            '0',
          )}；BP=0x${status.bpBits.toString(16)}；${status.writeProtected ? '已保护' : '未保护'}`,
        status.writeProtected ? 'warn' : 'success',
      )
    } catch (e: unknown) {
      store.addLog(`NOR 写保护检查失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  async function disableNorWriteProtect() {
    if (store.selectedType !== 'SPI_NOR' || !store.canOperate) {
      store.addLog('解除写保护仅支持已连接的 SPI NOR 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = '解除 NOR 写保护'
    try {
      const msg = (await call('nor_wp_disable')) as string
      store.addLog(msg, 'success')
    } catch (e: unknown) {
      store.addLog(`解除 NOR 写保护失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  async function scanBadBlocks(): Promise<{ totalBlocks: number; badBlocks: number[] }> {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('坏块扫描仅支持已连接的 SPI NAND 芯片', 'warn')
      throw new Error('bad block scan requires SPI NAND')
    }
    store.isRunning = true
    store.currentOp = '读取坏块'
    store.progress = 0
    store.progressMessage = '正在扫描坏块...'
    const opStart = Date.now()
    store.addLog('坏块扫描：实验性功能，开始执行', 'functionTest')
    const unlisten = await listenProgress('bad_block_progress', (done, total) => {
      const pct = total > 0 ? Math.floor((done / total) * 100) : 0
      store.progress = pct
      store.progressMessage = withEstimate(`坏块扫描 ${done} / ${total}`, opStart, done, total)
    })
    try {
      const result = (await call('scan_bad_blocks')) as {
        totalBlocks: number
        badBlocks: number[]
        badCount: number
      }
      store.progress = 100
      if (result.badCount === 0) {
        store.addLog(`坏块扫描完成：共 ${result.totalBlocks} 块，未发现坏块`, 'functionTest')
      } else {
        store.addLog(
          `坏块扫描完成：共 ${result.totalBlocks} 块，发现 ${result.badCount} 个坏块`,
          'functionTest',
        )
        const page = store.chipDetails?.page ?? 1
        const block = store.chipDetails?.block ?? page
        const pagesPerBlock =
          store.chipDetails?.pagesPerBlock ?? Math.max(1, Math.floor(block / page))
        const spare = store.chipDetails?.spare ?? 64
        result.badBlocks.forEach((blockNo, index) => {
          const main = blockNo * block
          const withOob = main + blockNo * pagesPerBlock * spare
          store.addLog(
            `坏块 ${index + 1}: 块号 0x${blockNo.toString(16)}, 主数据区起始 0x${main.toString(16)}, 含OOB起始 0x${withOob.toString(16)}`,
            'warn',
          )
        })
      }
      return result
    } catch (e: unknown) {
      store.addLog(`坏块扫描失败: ${String(e)}`, 'error')
      throw e
    } finally {
      unlisten()
      store.isRunning = false
      store.currentOp = ''
      store.progressMessage = ''
    }
  }

  // ── 全片擦除 ────────────────────────────────────────────────────────────────
  async function eraseChip() {
    store.isRunning = true
    store.currentOp = '全片擦除'
    store.progress = 0
    store.progressIndeterminate = true
    store.progressElapsedMs = 0
    store.progressMessage = '正在准备擦除...'
    store.addLog('开始全片擦除...')

    let unlistenErase: UnlistenFn | null = null
    let unlistenBadBlock: UnlistenFn | null = null
    try {
      unlistenErase = await onEvent<{
        done: number
        total: number
        phase: string
        message: string
        elapsedMs?: number | null
      }>('erase_progress', ({ payload }) => {
        if (payload.total > 0) {
          store.progressIndeterminate = false
          store.progressElapsedMs = 0
          store.progress = Math.min(99, Math.floor((payload.done / payload.total) * 100))
        } else {
          // 全片擦除只有忙/不忙状态：不编造百分比，只显示动画 + 计时器
          store.progressIndeterminate = true
          store.progressElapsedMs = payload.elapsedMs ?? 0
        }
        store.progressMessage = payload.message
      })
      // NAND 擦除前的坏块扫描复用 bad_block_progress 事件
      unlistenBadBlock = await onProgress<{ done: number; total: number }>(
        'bad_block_progress',
        (payload) => {
          store.progressIndeterminate = false
          store.progressElapsedMs = 0
          store.progress = Math.min(99, Math.floor((payload.done / payload.total) * 100))
          store.progressMessage = `正在扫描坏块 ${payload.done} / ${payload.total}`
        },
      )
      const msg = (await call('chip_erase', {
        badBlockMode: store.nandBadBlockMode,
      })) as string
      store.progressIndeterminate = false
      store.progressElapsedMs = 0
      store.progress = 100
      store.progressMessage = '擦除完成'
      store.addLog(msg, 'success')
      if (store.detectedChipSize > 0) {
        fillHexWithFF(store.detectedChipSize)
      }
      if (settings.blankCheckAfterErase) {
        await blankCheckChip()
      }
    } catch (e: unknown) {
      autoStepFailed = true
      store.progressIndeterminate = false
      store.progressElapsedMs = 0
      store.progress = 0
      store.progressMessage = ''
      store.addLog(`擦除失败: ${String(e)}`, 'error')
    } finally {
      unlistenErase?.()
      unlistenBadBlock?.()
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 查空 ────────────────────────────────────────────────────────────────
  async function blankCheckChip() {
    if (!store.chipDetected || store.detectedChipSize === 0) {
      store.addLog('请先检测芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = '查空'
    store.progress = 0
    store.progressIndeterminate = false
    store.progressElapsedMs = 0
    store.progressMessage = '正在查空...'
    store.addLog('开始查空...')

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('blank_check_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = `已检查 ${done} / ${total} 字节 (${pct}%)`
      })
      const result = (await call('blank_check', {
        size: store.detectedChipSize,
        startAddr: 0,
        badBlockMode: store.nandBadBlockMode,
      })) as { blank: boolean; checked: number; firstNonBlank: number | null }
      store.progress = 100
      if (result.blank) {
        store.progressMessage = '查空完成：芯片为空 (全 FF)'
        store.addLog(`查空完成：全部 ${result.checked} 字节均为 0xFF`, 'success')
      } else {
        store.progressMessage = '查空完成：芯片非空'
        store.addLog(
          `查空完成：首个非空地址 0x${(result.firstNonBlank ?? 0)
            .toString(16)
            .padStart(8, '0')
            .toUpperCase()}（已检查 ${result.checked} 字节）`,
          'warn',
        )
      }
    } catch (e: unknown) {
      autoStepFailed = true
      store.addLog(`查空失败: ${String(e)}`, 'error')
    } finally {
      unlisten?.()
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 读取芯片 ─────────────────────────────────────────────────────────────
  async function readChip() {
    if (!store.chipDetected || store.detectedChipSize === 0) {
      store.addLog('请先检测芯片', 'warn')
      return
    }
    if (store.selectedType === 'SPI_NAND' && store.nandReadBadBlockFirst) {
      try {
        await scanBadBlocks()
      } catch {
        autoStepFailed = true
        return
      }
    }
    store.isRunning = true
    store.currentOp = '读取芯片'
    store.progress = 0
    store.progressMessage = '准备读取...'
    const opStart = Date.now()
    store.addLog(`开始读取，容量 ${store.detectedChipSize} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('read_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = withEstimate(
          `已读取 ${done} / ${total} 字节 (${pct}%)`,
          opStart,
          done,
          total,
        )
      })
      // 后端用 tauri::ipc::Response 返回原始字节（ArrayBuffer），
      // 大容量 NAND（如 128MB）不再经过 JSON number[] 序列化，避免前端卡死。
      const raw = await call<ArrayBuffer>('read_chip', {
        size: store.detectedChipSize,
        startAddr: 0,
        badBlockMode: store.nandBadBlockMode,
      })
      const bytes = new Uint8Array(raw)
      store.hexData = bytes
      store.progress = 100
      store.progressMessage = '读取完成'
      store.addLog(`读取完成，共 ${bytes.length} 字节`, 'success')
    } catch (e: unknown) {
      autoStepFailed = true
      store.addLog(`读取失败: ${String(e)}`, 'error')
      store.progress = 0
      store.progressMessage = ''
    } finally {
      unlisten?.()
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 写入芯片 ────────────────────────────────────────────────────────────────
  // 将 HexViewer 中当前的 hexData 写入芯片。
  // 写前应由用户自行完成擦除（Page Program 只能将 1 写成 0，不能反向）。
  async function writeChip(forceSegmented = false) {
    if (!store.hexData || store.hexData.length === 0) {
      store.addLog('HexViewer 中没有数据，请先加载文件或读取芯片', 'warn')
      return
    }
    if (!forceSegmented && store.selectedType === 'SPI_NAND' && store.nandReadBadBlockFirst) {
      try {
        const scan = await scanBadBlocks()
        if (scan.badCount > 0) {
          store.addLog('坏块扫描属于提示信息：写入会按所选坏块模式继续执行', 'info')
        }
      } catch {
        autoStepFailed = true
        return
      }
    }
    const payload = store.hexData
    store.isRunning = true
    store.currentOp = '写入芯片'
    store.progress = 0
    store.progressMessage = '准备写入...'
    const opStart = Date.now()
    store.addLog(`开始写入，${payload.length} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('write_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = withEstimate(
          `已写入 ${done} / ${total} 字节 (${pct}%)`,
          opStart,
          done,
          total,
        )
      })
      // 顶层 Uint8Array 会作为原始字节体（application/octet-stream）交给
      // tauri::ipc::Request，不再走 JSON number[]，128MB 镜像也能保持流畅。
      const headers: Record<string, string> = {
        'x-start-addr': '0',
        'x-force-segmented': String(forceSegmented),
        'x-bad-block-mode': store.nandBadBlockMode,
      }
      const msg = await call<string>('write_chip', payload, { headers })
      store.progress = 100
      store.progressMessage = '写入完成'
      store.addLog(msg, 'success')
    } catch (e: unknown) {
      autoStepFailed = true
      const message = String(e)
      if (!forceSegmented && message.includes('SPI_PAGE_TOO_LARGE')) {
        // CH341 DLL 单帧放不下大页 NAND：先给用户清晰警告
        store.addLog(message, 'warn')
        if (window.confirm('仍要强制尝试分段写入吗？（实验性路径，未经过真机验证）')) {
          unlisten?.()
          await writeChip(true)
          return
        }
        store.addLog('已取消写入。建议改用 CH347，或使用 libusb 后端（两者可正常写大页）', 'warn')
        store.progress = 0
        store.progressMessage = ''
      } else {
        store.addLog(`写入失败: ${String(e)}`, 'error')
        store.progress = 0
        store.progressMessage = ''
      }
    } finally {
      unlisten?.()
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 校验芯片 ────────────────────────────────────────────────────────────────
  // 读回芯片内容，与 HexViewer 中的 hexData 逐字节对比。
  async function verifyChip() {
    if (!store.hexData || store.hexData.length === 0) {
      store.addLog('HexViewer 中没有数据，无法校验', 'warn')
      return
    }
    const payload = store.hexData
    store.isRunning = true
    store.currentOp = '校验芯片'
    store.progress = 0
    store.progressMessage = '准备校验...'
    const opStart = Date.now()
    store.addLog(`开始校验，${payload.length} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('verify_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = withEstimate(
          `已校验 ${done} / ${total} 字节 (${pct}%)`,
          opStart,
          done,
          total,
        )
      })
      // 与写入一致：顶层 Uint8Array 走原始字节通道，避免大镜像 JSON 卡顿。
      const headers: Record<string, string> = {
        'x-start-addr': '0',
        'x-bad-block-mode': store.nandBadBlockMode,
      }
      const msg = await call<string>('verify_chip', payload, { headers })
      store.progress = 100
      store.progressMessage = '校验完成'
      store.addLog(msg, 'success')
      if (store.nandCheckSoundSwitch) {
        playVerifySound()
      }
    } catch (e: unknown) {
      autoStepFailed = true
      // 后端把首个不一致地址和字节值放在错误信息里，直接显示即可
      store.addLog(`校验失败: ${String(e)}`, 'error')
      store.progress = 0
      store.progressMessage = ''
    } finally {
      unlisten?.()
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 保存 HexViewer 内容到文件 ───────────────────────────────────────────────
  // 原生路径：Rust 端弹出 Windows 原生对话框 + std::fs 写盘（无插件依赖）。
  // 兜底路径：纯前端 Blob + <a> 触发下载。
  // format: 'bin' 直接写二进制；'hex' 输出 Intel HEX 格式（:llaaaatt[dd...]cc）

  function buildIntelHex(data: Uint8Array): string {
    const lines: string[] = []
    const RECORD_LEN = 16
    for (let addr = 0; addr < data.length; addr += RECORD_LEN) {
      const chunk = data.slice(addr, addr + RECORD_LEN)
      const ll = chunk.length
      const addrHi = (addr >> 8) & 0xff
      const addrLo = addr & 0xff

      let sum = ll + addrHi + addrLo + 0x00 // type=00
      for (const b of chunk) sum += b
      const cc = (~sum + 1) & 0xff

      const ddStr = Array.from(chunk)
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('')
      lines.push(
        `:${ll.toString(16).padStart(2, '0')}${addrHi.toString(16).padStart(2, '0')}${addrLo.toString(16).padStart(2, '0')}00${ddStr}${cc.toString(16).padStart(2, '0')}`.toUpperCase(),
      )
    }
    lines.push(':00000001FF')
    return lines.join('\r\n') + '\r\n'
  }

  function stemForSave(): string {
    if (store.filePath) {
      const name = store.filePath.split(/[\\/]/).pop() ?? store.filePath
      return name.replace(/\.[^.]+$/, '')
    }
    return store.selectedModel || 'dump'
  }

  async function saveFileNative(format: 'bin' | 'hex') {
    if (!store.hexData || store.hexData.length === 0) {
      store.addLog('HexViewer 中没有数据，无法保存', 'warn')
      return
    }
    const defaultName = `${stemForSave()}.${format}`
    try {
      const path = await call<string | null>('save_file_dialog', {
        defaultName,
        defaultExt: format,
      })
      if (!path) return

      let bytes: Uint8Array
      if (format === 'bin') {
        bytes = store.hexData
      } else {
        bytes = new TextEncoder().encode(buildIntelHex(store.hexData))
      }
      await call('write_file', { path, data: Array.from(bytes) })
      store.addLog(`已保存: ${path}`, 'success')
    } catch (e: unknown) {
      store.addLog(`保存失败: ${String(e)}`, 'error')
    }
  }

  function saveFile(format: 'bin' | 'hex') {
    if (!store.hexData || store.hexData.length === 0) {
      store.addLog('HexViewer 中没有数据，无法保存', 'warn')
      return
    }

    let blob: Blob
    const stem = stemForSave()

    if (format === 'bin') {
      blob = new Blob([store.hexData], { type: 'application/octet-stream' })
    } else {
      blob = new Blob([buildIntelHex(store.hexData)], { type: 'text/plain' })
    }

    const filename = `${stem}.${format}`
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
    store.addLog(`已保存: ${filename}`, 'success')
  }

  // ── 自动操作 ────────────────────────────────────────────────────────────────
  type AutoStepKey = 'read' | 'erase' | 'blankCheck' | 'write' | 'verify'

  function selectedAutoSteps(): AutoStepKey[] {
    const valid = new Set<AutoStepKey>(['read', 'erase', 'blankCheck', 'write', 'verify'])
    return settings.autoOrder
      .split(',')
      .map((step) => step.trim())
      .filter((step): step is AutoStepKey => valid.has(step as AutoStepKey))
  }

  async function runAuto(): Promise<boolean> {
    const steps = selectedAutoSteps()
    if (steps.length === 0) {
      store.addLog('未设置自动化流程', 'error')
      return false
    }
    const needsData = steps.includes('write') || steps.includes('verify')
    if (needsData && (!store.hexData || store.hexData.length === 0)) {
      store.addLog('自动流程包含写入/校验，但缓冲区没有数据；请先读取芯片或加载文件', 'warn')
      return false
    }

    const labels: Record<AutoStepKey, string> = {
      read: '读取',
      erase: '擦除',
      blankCheck: '查空',
      write: '写入',
      verify: '校验',
    }
    store.isRunning = true
    try {
      for (let i = 0; i < steps.length; i += 1) {
        const step = steps[i]
        store.currentOp = `自动操作 ${i + 1}/${steps.length}`
        store.progress = 0
        store.progressMessage = `${labels[step]}...`
        store.addLog(`自动步骤 ${i + 1}/${steps.length}: ${labels[step]}`)

        autoStepFailed = false
        switch (step) {
          case 'read':
            await readChip()
            break
          case 'erase':
            await eraseChip()
            break
          case 'blankCheck':
            await blankCheckChip()
            break
          case 'write':
            await writeChip()
            break
          case 'verify':
            await verifyChip()
            break
        }
        store.isRunning = true
        if (autoStepFailed) {
          store.addLog(`自动流程在第 ${i + 1} 步（${labels[step]}）失败，已停止`, 'error')
          break
        }
      }
      if (!autoStepFailed) {
        store.progress = 100
        store.progressMessage = '自动操作完成'
        store.addLog('自动操作完成', 'success')
      }
      return !autoStepFailed
    } finally {
      autoStepFailed = false
      store.isRunning = false
      store.currentOp = ''
      store.progressIndeterminate = false
    }
  }

  return {
    detectChip,
    autoDetectChip,
    cancelChipAutoDetect,
    checkNorWriteProtect,
    disableNorWriteProtect,
    scanBadBlocks,
    readNandUid,
    readNandParamPage,
    readNandBbmLut,
    readNandOtpPage,
    setNandEcc,
    readAt45PageMode,
    setAt45PageMode,
    eraseChip,
    blankCheckChip,
    readChip,
    writeChip,
    verifyChip,
    runAuto,
    selectedAutoSteps,
    saveFile,
    saveFileNative,
  }
}
