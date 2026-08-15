import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useProgStore, type DetectedChipInfo, formatBytes } from '@/stores/prog'

export function useSpiNor() {
  const store = useProgStore()

  // 预填充上限：NAND 等大容量芯片不预分配 FF，避免 WebView 内存爆炸
  const MAX_PREFILL = 8 * 1024 * 1024

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
    onProgress: (done: number, total: number) => void,
  ): Promise<UnlistenFn> {
    return listen<{ done: number; total: number }>(eventName, ({ payload }) => {
      onProgress(payload.done, payload.total)
    })
  }

  // ── 检测芯片 ────────────────────────────────────────────────────────────────
  async function detectChip() {
    store.detectStatus = 'detecting'
    store.currentOp = '检测芯片'
    store.addLog('正在检测 SPI Flash...')
    try {
      const result = (await invoke('detect_chip')) as {
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
    try {
      const result = (await invoke(command)) as { length: number; hex: string }
      store.addLog(`${label} 完成（${result.length} 字节，实验性命令，需真机验证）`, 'success')
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
    try {
      const result = (await invoke('read_nand_bbm_lut')) as {
        length: number
        hex: string
        entries: { index: number; lba: number; pba: number; free: boolean; valid: boolean }[]
      }
      store.addLog(`BBM 映射表读取完成（${result.length} 字节，实验性）`, 'success')
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

  async function setNandEcc(enable: boolean) {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('ECC 设置仅支持已连接的 SPI NAND 芯片', 'warn')
      return
    }
    store.isRunning = true
    store.currentOp = enable ? '开启硬件 ECC' : '关闭硬件 ECC'
    try {
      const enabled = (await invoke('set_nand_ecc', { enable })) as boolean
      store.addLog(`芯片内置 ECC 已${enabled ? '开启' : '关闭'}（实验性，需真机验证）`, 'success')
    } catch (e: unknown) {
      store.addLog(`ECC 设置失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
    }
  }

  // ── 坏块扫描（SPI NAND）────────────────────────────────────────────────────
  async function scanBadBlocks(): Promise<{ totalBlocks: number; badBlocks: number[] }> {
    if (store.selectedType !== 'SPI_NAND' || !store.canOperate) {
      store.addLog('坏块扫描仅支持已连接的 SPI NAND 芯片', 'warn')
      throw new Error('bad block scan requires SPI NAND')
    }
    store.isRunning = true
    store.currentOp = '读取坏块'
    store.progress = 0
    store.progressMessage = '正在扫描坏块...'
    const unlisten = await listenProgress('bad_block_progress', (done, total) => {
      const pct = total > 0 ? Math.floor((done / total) * 100) : 0
      store.progress = pct
      store.progressMessage = `坏块扫描 ${done} / ${total}`
    })
    try {
      const result = (await invoke('scan_bad_blocks')) as {
        totalBlocks: number
        badBlocks: number[]
        badCount: number
      }
      store.progress = 100
      if (result.badCount === 0) {
        store.addLog(`坏块扫描完成：共 ${result.totalBlocks} 块，未发现坏块`, 'success')
      } else {
        store.addLog(
          `坏块扫描完成：共 ${result.totalBlocks} 块，发现 ${result.badCount} 个坏块`,
          'warn',
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
    store.progressMessage = '正在擦除...'
    store.addLog('开始全片擦除...')
    try {
      const msg = (await invoke('chip_erase', {
        badBlockMode: store.nandBadBlockMode,
      })) as string
      store.addLog(msg, 'success')
      if (store.detectedChipSize > 0) {
        fillHexWithFF(store.detectedChipSize)
      }
      store.progress = 100
    } catch (e: unknown) {
      store.addLog(`擦除失败: ${String(e)}`, 'error')
    } finally {
      store.isRunning = false
      store.currentOp = ''
      store.progress = 0
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
        return
      }
    }
    store.isRunning = true
    store.currentOp = '读取芯片'
    store.progress = 0
    store.progressMessage = '准备读取...'
    store.addLog(`开始读取，容量 ${store.detectedChipSize} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('read_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = `已读取 ${done} / ${total} 字节 (${pct}%)`
      })
      const raw = await invoke<number[]>('read_chip', {
        size: store.detectedChipSize,
        startAddr: 0,
        badBlockMode: store.nandBadBlockMode,
      })
      store.hexData = new Uint8Array(raw)
      store.progress = 100
      store.progressMessage = '读取完成'
      store.addLog(`读取完成，共 ${raw.length} 字节`, 'success')
    } catch (e: unknown) {
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
        await scanBadBlocks()
      } catch {
        return
      }
    }
    store.isRunning = true
    store.currentOp = '写入芯片'
    store.progress = 0
    store.progressMessage = '准备写入...'
    store.addLog(`开始写入，${store.hexData.length} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('write_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = `已写入 ${done} / ${total} 字节 (${pct}%)`
      })
      // Tauri 传 Vec<u8> 需要 Array<number>，从 Uint8Array 转一下
      const msg = await invoke<string>('write_chip', {
        data: Array.from(store.hexData),
        startAddr: 0,
        forceSegmented,
        badBlockMode: store.nandBadBlockMode,
      })
      store.progress = 100
      store.progressMessage = '写入完成'
      store.addLog(msg, 'success')
    } catch (e: unknown) {
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
    store.isRunning = true
    store.currentOp = '校验芯片'
    store.progress = 0
    store.progressMessage = '准备校验...'
    store.addLog(`开始校验，${store.hexData.length} 字节...`)

    let unlisten: UnlistenFn | null = null
    try {
      unlisten = await listenProgress('verify_progress', (done, total) => {
        const pct = Math.floor((done / total) * 100)
        store.progress = pct
        store.progressMessage = `已校验 ${done} / ${total} 字节 (${pct}%)`
      })
      const msg = await invoke<string>('verify_chip', {
        data: Array.from(store.hexData),
        startAddr: 0,
        badBlockMode: store.nandBadBlockMode,
      })
      store.progress = 100
      store.progressMessage = '校验完成'
      store.addLog(msg, 'success')
    } catch (e: unknown) {
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
      const path = await invoke<string | null>('save_file_dialog', {
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
      await invoke('write_file', { path, data: Array.from(bytes) })
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

  return {
    detectChip,
    scanBadBlocks,
    readNandUid,
    readNandParamPage,
    readNandBbmLut,
    setNandEcc,
    eraseChip,
    readChip,
    writeChip,
    verifyChip,
    saveFile,
    saveFileNative,
  }
}
