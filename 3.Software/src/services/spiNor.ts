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
    buf.fill(0xFF)
    store.hexData = buf
  }

  // ── 通用：订阅一个进度 event，返回取消订阅函数 ──────────────────────────────
  async function listenProgress(
    eventName: string,
    onProgress: (done: number, total: number) => void
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
      const result = await invoke('detect_chip') as { text: string; info: DetectedChipInfo | null }
      store.detectStatus = 'success'
      store.chipInfo = result.text.split('\n')
      store.addLog(result.text, 'success')
      if (result.info) {
        store.selectedType = result.info.protocol
        store.chipVendors = await store.loadChipVendorsDirect(result.info.protocol)
        store.selectedVendor = result.info.vendor
        store.chipModels = await store.loadChipModelsDirect(result.info.protocol, result.info.vendor)
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
    } catch (e: any) {
      store.detectStatus = 'chip_rule_fail'
      store.chipInfo = []
      store.addLog(`检测失败: ${e}`, 'error')
      store.chipDetected = false
      store.detectedChipSize = 0
      store.chipDetails = null
      if (typeof e === 'string' && e.includes('传输失败')) {
        store.status = 'error'
        store.connectedDevice = ''
      }
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
      const msg = await invoke('chip_erase') as string
      store.addLog(msg, 'success')
      if (store.detectedChipSize > 0) {
        fillHexWithFF(store.detectedChipSize)
      }
      store.progress = 100
    } catch (e: any) {
      store.addLog(`擦除失败: ${e}`, 'error')
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
      })
      store.hexData = new Uint8Array(raw)
      store.progress = 100
      store.progressMessage = '读取完成'
      store.addLog(`读取完成，共 ${raw.length} 字节`, 'success')
    } catch (e: any) {
      store.addLog(`读取失败: ${e}`, 'error')
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
      })
      store.progress = 100
      store.progressMessage = '写入完成'
      store.addLog(msg, 'success')
    } catch (e: any) {
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
        store.addLog(`写入失败: ${e}`, 'error')
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
      })
      store.progress = 100
      store.progressMessage = '校验完成'
      store.addLog(msg, 'success')
    } catch (e: any) {
      // 后端把首个不一致地址和字节值放在错误信息里，直接显示即可
      store.addLog(`校验失败: ${e}`, 'error')
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
      const addrHi = (addr >> 8) & 0xFF
      const addrLo = addr & 0xFF

      let sum = ll + addrHi + addrLo + 0x00 // type=00
      for (const b of chunk) sum += b
      const cc = (~sum + 1) & 0xFF

      const ddStr = Array.from(chunk).map(b => b.toString(16).padStart(2, '0')).join('')
      lines.push(
        `:${ll.toString(16).padStart(2, '0')}${addrHi.toString(16).padStart(2, '0')}${addrLo.toString(16).padStart(2, '0')}00${ddStr}${cc.toString(16).padStart(2, '0')}`.toUpperCase()
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
    } catch (e: any) {
      store.addLog(`保存失败: ${e}`, 'error')
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
    eraseChip,
    readChip,
    writeChip,
    verifyChip,
    saveFile,
    saveFileNative,
  }
}
