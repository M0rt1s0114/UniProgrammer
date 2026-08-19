<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { t } from './i18n'

const props = defineProps<{
  data: Uint8Array | null
  baseAddr?: number
}>()

const emit = defineEmits<{
  edit: [offset: number, value: number]
  replace: [data: Uint8Array]
  log: [level: string, message: string]
}>()

// Plugin-local working copy. The host keeps its own authoritative buffer;
// mutations never touch `props.data` directly.
const buf = ref<Uint8Array | null>(null)
const viewData = computed(() => buf.value)
// Per-byte edits mutate `buf` in place, so rendering needs this explicit
// dependency bump instead of copying the whole buffer on every keystroke.
const editTick = ref(0)

const BYTES_PER_ROW = 16
const ROW_HEIGHT = 20

// 列宽
const colWidths = ref({ addr: 80, bytes: 22 * 16 + 20, ascii: 130 })

// 拖拽相关
const dragging = ref<'addr-bytes' | 'bytes-ascii' | null>(null)
const dragStartX = ref(0)
const dragStartWidthA = ref(0)
const dragStartWidthB = ref(0)

function startDrag(type: 'addr-bytes' | 'bytes-ascii', event: MouseEvent) {
  dragging.value = type
  dragStartX.value = event.clientX
  if (type === 'addr-bytes') {
    dragStartWidthA.value = colWidths.value.addr
    dragStartWidthB.value = colWidths.value.bytes
  } else {
    dragStartWidthA.value = colWidths.value.bytes
    dragStartWidthB.value = colWidths.value.ascii
  }
  document.addEventListener('mousemove', onDrag)
  document.addEventListener('mouseup', stopDrag)
}

function onDrag(event: MouseEvent) {
  const delta = event.clientX - dragStartX.value
  if (dragging.value === 'addr-bytes') {
    let newAddr = dragStartWidthA.value + delta
    let newBytes = dragStartWidthB.value - delta
    if (newAddr < 60) newAddr = 60
    if (newBytes < 200) newBytes = 200
    colWidths.value.addr = newAddr
    colWidths.value.bytes = newBytes
  } else if (dragging.value === 'bytes-ascii') {
    let newBytes = dragStartWidthA.value + delta
    let newAscii = dragStartWidthB.value - delta
    if (newBytes < 200) newBytes = 200
    if (newAscii < 80) newAscii = 80
    colWidths.value.bytes = newBytes
    colWidths.value.ascii = newAscii
  }
}

function stopDrag() {
  dragging.value = null
  document.removeEventListener('mousemove', onDrag)
  document.removeEventListener('mouseup', stopDrag)
}

// ── 编辑 ────────────────────────────────────────────────────────────────────
const editing = ref<{ row: number; col: number; text: string } | null>(null)

type EditPatch =
  | { kind: 'byte'; offset: number; before: number; after: number }
  | { kind: 'buffer'; before: Uint8Array; after: Uint8Array }

const history = ref<EditPatch[]>([])

function pushHistory(patch: EditPatch) {
  history.value.push(patch)
  if (history.value.length > 50) history.value.shift()
}

function byteIndex(row: number, col: number) {
  return row * BYTES_PER_ROW + col
}

function startEdit(row: number, col: number, text: string) {
  if (!buf.value || text === '  ') return
  editing.value = { row, col, text }
}

function commitEdit() {
  const e = editing.value
  editing.value = null
  const current = buf.value
  if (!e || !current) return
  const text = e.text.trim()
  if (!/^[0-9a-fA-F]{1,2}$/.test(text)) return
  const value = parseInt(text, 16)
  const idx = byteIndex(e.row, e.col)
  if (idx >= current.length) return
  const before = current[idx]
  if (before === value) return

  current[idx] = value
  editTick.value++
  pushHistory({ kind: 'byte', offset: idx, before, after: value })
  emit('edit', idx, value)
}

function undo() {
  const patch = history.value.pop()
  const current = buf.value
  if (!patch || !current) return
  if (patch.kind === 'byte') {
    if (patch.offset >= current.length) return
    current[patch.offset] = patch.before
    editTick.value++
    emit('edit', patch.offset, patch.before)
    return
  }
  buf.value = patch.before
  emit('replace', patch.before)
}

// ── 搜索 ────────────────────────────────────────────────────────────────────
const searchText = ref('')
const matchStart = ref(-1)
const matchLen = ref(0)

function parseHexPattern(text: string): number[] | null {
  const tokens = text
    .trim()
    .split(/[\s,]+/)
    .filter(Boolean)
  if (tokens.length === 0) return null
  const bytes: number[] = []
  for (const token of tokens) {
    if (!/^[0-9a-fA-F]{1,2}$/.test(token)) return null
    bytes.push(parseInt(token, 16))
  }
  return bytes
}

function findNext() {
  const current = buf.value
  if (!current) return
  const pattern = parseHexPattern(searchText.value)
  if (!pattern || pattern.length === 0) return
  const start = matchStart.value + 1
  for (let i = start; i <= current.length - pattern.length; i++) {
    let ok = true
    for (let j = 0; j < pattern.length; j++) {
      if (current[i + j] !== pattern[j]) {
        ok = false
        break
      }
    }
    if (ok) {
      matchStart.value = i
      matchLen.value = pattern.length
      scrollToRow(Math.floor(i / BYTES_PER_ROW))
      emit('log', 'info', `${t('hex.foundAt')}0x${i.toString(16).padStart(8, '0').toUpperCase()}`)
      return
    }
  }
  // 从头再来一轮
  for (let i = 0; i < Math.min(start, current.length - pattern.length + 1); i++) {
    let ok = true
    for (let j = 0; j < pattern.length; j++) {
      if (current[i + j] !== pattern[j]) {
        ok = false
        break
      }
    }
    if (ok) {
      matchStart.value = i
      matchLen.value = pattern.length
      scrollToRow(Math.floor(i / BYTES_PER_ROW))
      emit('log', 'info', `${t('hex.foundAt')}0x${i.toString(16).padStart(8, '0').toUpperCase()}`)
      return
    }
  }
  matchStart.value = -1
  matchLen.value = 0
  emit('log', 'warn', t('hex.notFound'))
}

function isHit(row: number, col: number) {
  if (matchStart.value < 0) return false
  const idx = byteIndex(row, col)
  return idx >= matchStart.value && idx < matchStart.value + matchLen.value
}

// ── 跳转 / 填充 / 校验和 ─────────────────────────────────────────────────────
const gotoText = ref('')
const fillText = ref('FF')

function scrollToRow(row: number) {
  const target = scrollTargetForRow(row)
  scrollTop.value = target
  nextTick(() => {
    if (containerRef.value) containerRef.value.scrollTop = target
  })
}

function gotoAddress() {
  const current = buf.value
  if (!current) return
  const addr = parseInt(gotoText.value.trim(), 16)
  if (Number.isNaN(addr) || addr < 0 || addr >= current.length) {
    emit('log', 'warn', `地址超出范围: ${gotoText.value}`)
    return
  }
  scrollToRow(Math.floor(addr / BYTES_PER_ROW))
}

function fillBuffer() {
  const current = buf.value
  if (!current) return
  const pattern = parseHexPattern(fillText.value)
  if (!pattern || pattern.length === 0) return
  const next = new Uint8Array(current.length)
  for (let i = 0; i < next.length; i++) {
    next[i] = pattern[i % pattern.length]
  }
  pushHistory({ kind: 'buffer', before: current, after: next })
  buf.value = next
  emit('replace', next)
  emit('log', 'success', t('hex.fillDone'))
}

let crcTable: Uint32Array | null = null

function crc32(data: Uint8Array): number {
  if (!crcTable) {
    crcTable = new Uint32Array(256)
    for (let n = 0; n < 256; n++) {
      let c = n
      for (let k = 0; k < 8; k++) {
        c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
      }
      crcTable[n] = c >>> 0
    }
  }
  let crc = 0xffffffff
  for (let i = 0; i < data.length; i++) {
    crc = (crcTable[(crc ^ data[i]) & 0xff]! ^ (crc >>> 8)) >>> 0
  }
  return (crc ^ 0xffffffff) >>> 0
}

function checksum() {
  const current = buf.value
  if (!current) return
  let sum8 = 0
  let sum16 = 0
  let xor8 = 0
  for (let i = 0; i < current.length; i++) {
    sum8 = (sum8 + current[i]) & 0xff
    sum16 = (sum16 + current[i]) & 0xffff
    xor8 ^= current[i]
  }
  const crc = crc32(current)
  emit(
    'log',
    'info',
    `Checksum: SUM8=0x${sum8.toString(16).padStart(2, '0').toUpperCase()} ` +
      `SUM16=0x${sum16.toString(16).padStart(4, '0').toUpperCase()} ` +
      `XOR8=0x${xor8.toString(16).padStart(2, '0').toUpperCase()} ` +
      `CRC32=0x${crc.toString(16).padStart(8, '0').toUpperCase()}`,
  )
}

// ── 虚拟滚动 ────────────────────────────────────────────────────────────────
const scrollTop = ref(0)
const containerRef = ref<HTMLElement | null>(null)
const containerHeight = ref(0)

const totalRows = computed(() => (buf.value ? Math.ceil(buf.value.length / BYTES_PER_ROW) : 0))
const totalHeight = computed(() => totalRows.value * ROW_HEIGHT)

// 浏览器会钳制超高的滚动容器（Chromium 上限约 3350 万 px）。
// 128MB 镜像有 800 多万行、真实高度 1.6 亿 px，会被截断到无法滚到末尾，
// 因此大缓冲区使用等比缩放的虚拟高度。
const MAX_VIRTUAL_HEIGHT = 30_000_000
const usesScaledScroll = computed(() => totalHeight.value > MAX_VIRTUAL_HEIGHT)
const viewportHeight = computed(() => Math.min(totalHeight.value, MAX_VIRTUAL_HEIGHT))

function scrollTargetForRow(row: number): number {
  const lastRow = Math.max(0, totalRows.value - 1)
  const clamped = Math.max(0, Math.min(row, lastRow))
  if (!usesScaledScroll.value || lastRow === 0) return clamped * ROW_HEIGHT
  return Math.floor((clamped / lastRow) * MAX_VIRTUAL_HEIGHT)
}

function rowForScroll(scroll: number): number {
  if (!usesScaledScroll.value) return Math.floor(scroll / ROW_HEIGHT)
  if (totalRows.value <= 1) return 0
  const ratio = Math.max(0, Math.min(1, scroll / MAX_VIRTUAL_HEIGHT))
  return Math.floor(ratio * (totalRows.value - 1))
}

const visibleRowCount = computed(() => {
  if (containerHeight.value <= 0) return 22
  return Math.ceil(containerHeight.value / ROW_HEIGHT) + 8
})

const startRow = computed(() => Math.max(0, rowForScroll(scrollTop.value) - 4))
const endRow = computed(() => Math.min(totalRows.value, startRow.value + visibleRowCount.value))

interface HexRow {
  addr: string
  bytes: string[]
  ascii: string
  rowIndex: number
}

const visibleRows = computed<HexRow[]>(() => {
  // `editTick` is an explicit dependency so in-place byte edits re-render
  // without copying the whole buffer.
  void editTick.value
  const current = buf.value
  if (!current) return []
  const rows: HexRow[] = []
  const base = props.baseAddr ?? 0
  for (let r = startRow.value; r < endRow.value; r++) {
    const offset = r * BYTES_PER_ROW
    const chunk = current.slice(offset, offset + BYTES_PER_ROW)
    const bytes: string[] = []
    let ascii = ''
    for (let i = 0; i < BYTES_PER_ROW; i++) {
      if (i < chunk.length) {
        const b = chunk[i]
        bytes.push(b.toString(16).padStart(2, '0').toUpperCase())
        ascii += b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '·'
      } else {
        bytes.push('  ')
        ascii += ' '
      }
    }
    rows.push({
      addr: (base + offset).toString(16).padStart(8, '0').toUpperCase(),
      bytes,
      ascii,
      rowIndex: r,
    })
  }
  return rows
})

const paddingTop = computed(() =>
  usesScaledScroll.value ? scrollTargetForRow(startRow.value) : startRow.value * ROW_HEIGHT,
)

function onScroll(e: Event) {
  scrollTop.value = (e.target as HTMLElement).scrollTop
}

function isEditing(row: number, col: number) {
  return !!editing.value && editing.value.row === row && editing.value.col === col
}

// 外部重置滚动位置
function scrollToTop() {
  scrollTop.value = 0
  nextTick(() => {
    if (containerRef.value) {
      containerRef.value.scrollTop = 0
    }
  })
}

defineExpose({ scrollToTop })

watch(
  () => props.data,
  (next) => {
    buf.value = next
    editing.value = null
    matchStart.value = -1
    matchLen.value = 0
    history.value = []
    editTick.value++
    scrollToTop()
  },
  { immediate: true },
)

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  if (containerRef.value) {
    containerHeight.value = containerRef.value.clientHeight
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const h = entry.contentRect.height
        if (h > 0) containerHeight.value = h
      }
    })
    resizeObserver.observe(containerRef.value)
  }
})

onUnmounted(() => {
  resizeObserver?.disconnect()
})
</script>

<template>
  <div class="hex-viewer">
    <div
      class="hex-header"
      :style="{
        '--addr-w': colWidths.addr + 'px',
        '--bytes-w': colWidths.bytes + 'px',
        '--ascii-w': colWidths.ascii + 'px',
      }"
    >
      <span class="col-addr" :style="{ width: colWidths.addr + 'px' }">{{ t('hex.address') }}</span>
      <div class="drag-handle" @mousedown.prevent="startDrag('addr-bytes', $event)"></div>
      <span class="col-bytes" :style="{ width: colWidths.bytes + 'px' }">
        <span v-for="i in 16" :key="i" class="byte-label">
          {{ (i - 1).toString(16).padStart(2, '0').toUpperCase() }}
        </span>
      </span>
      <div class="drag-handle" @mousedown.prevent="startDrag('bytes-ascii', $event)"></div>
      <span class="col-ascii" :style="{ width: colWidths.ascii + 'px' }">{{ t('hex.ascii') }}</span>
      <div class="header-spacer" />
      <span class="edit-hint">{{ t('hex.editHint') }}</span>
    </div>

    <div class="hex-toolbar">
      <div class="tool-group">
        <span class="tool-label">{{ t('hex.search') }}</span>
        <input
          v-model="searchText"
          class="tool-input"
          :placeholder="t('hex.searchPlaceholder')"
          @keydown.enter="findNext"
        />
        <button class="btn btn-ghost btn-sm" @click="findNext">»</button>
      </div>
      <div class="tool-group">
        <span class="tool-label">{{ t('hex.goto') }}</span>
        <input
          v-model="gotoText"
          class="tool-input tool-input-addr"
          :placeholder="t('hex.gotoPlaceholder')"
          @keydown.enter="gotoAddress"
        />
        <button class="btn btn-ghost btn-sm" @click="gotoAddress">Go</button>
      </div>
      <div class="tool-group">
        <span class="tool-label">{{ t('hex.fill') }}</span>
        <input
          v-model="fillText"
          class="tool-input tool-input-fill"
          :placeholder="t('hex.fillPlaceholder')"
        />
        <button class="btn btn-ghost btn-sm" @click="fillBuffer">Fill</button>
      </div>
      <button class="btn btn-ghost btn-sm" :disabled="history.length === 0" @click="undo">
        {{ t('hex.undo') }}
      </button>
      <button class="btn btn-ghost btn-sm" @click="checksum">{{ t('hex.checksum') }}</button>
    </div>

    <div ref="containerRef" class="hex-scroll" @scroll="onScroll">
      <div v-if="!viewData || viewData.length === 0" class="hex-empty">
        <svg
          width="32"
          height="32"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14,2 14,8 20,8" />
        </svg>
        <span>{{ t('hex.noData') }}</span>
      </div>

      <div v-else :style="{ height: viewportHeight + 'px', position: 'relative' }">
        <div :style="{ paddingTop: paddingTop + 'px' }">
          <div
            v-for="row in visibleRows"
            :key="row.rowIndex"
            class="hex-row"
            :style="{ height: ROW_HEIGHT + 'px' }"
          >
            <span class="col-addr addr-text" :style="{ width: colWidths.addr + 'px' }">{{
              row.addr
            }}</span>
            <span class="col-bytes" :style="{ width: colWidths.bytes + 'px' }">
              <span
                v-for="(byte, bi) in row.bytes"
                :key="bi"
                class="byte-cell"
                :class="{
                  'byte-null': byte === '00',
                  'byte-ff': byte === 'FF',
                  'byte-pad': byte === '  ',
                  'byte-hit': isHit(row.rowIndex, bi),
                  'byte-editing': isEditing(row.rowIndex, bi),
                }"
                @click="startEdit(row.rowIndex, bi, byte)"
              >
                <input
                  v-if="isEditing(row.rowIndex, bi)"
                  v-model="editing.text"
                  class="byte-edit"
                  maxlength="2"
                  @click.stop
                  @keydown.enter="commitEdit"
                  @blur="commitEdit"
                />
                <template v-else>{{ byte }}</template>
              </span>
            </span>
            <span class="col-ascii ascii-text" :style="{ width: colWidths.ascii + 'px' }">{{
              row.ascii
            }}</span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="viewData && viewData.length" class="hex-footer">
      <span>{{ viewData.length.toLocaleString() }} {{ t('hex.bytes') }}</span>
      <span>{{ Math.ceil(viewData.length / 1024).toLocaleString() }} {{ t('hex.kb') }}</span>
      <span>{{ totalRows.toLocaleString() }} {{ t('hex.rows') }}</span>
    </div>
  </div>
</template>

<style scoped>
.hex-viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-base);
  border-radius: var(--radius-md);
  overflow: hidden;
  font-family: var(--font-mono);
  font-size: 12px;
}

.hex-header {
  display: flex;
  align-items: center;
  padding: 6px 10px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0;
  text-transform: uppercase;
  color: var(--text-muted);
  flex-shrink: 0;
  user-select: none;
}

.header-spacer {
  flex: 1;
}
.edit-hint {
  text-transform: none;
  font-weight: 400;
  font-size: 10px;
  color: var(--text-muted);
}

.col-addr {
  text-align: right;
  flex-shrink: 0;
}

.col-bytes {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
  overflow: hidden;
}

.byte-label {
  width: 22px;
  text-align: center;
  color: var(--text-muted);
}

.col-ascii {
  flex-shrink: 0;
  padding-left: 12px;
  border-left: 1px solid var(--border);
}

.drag-handle {
  width: 5px;
  cursor: col-resize;
  height: 100%;
  background: transparent;
  flex-shrink: 0;
  transition: background 120ms;
  margin: 0 2px;
}
.drag-handle:hover {
  background: var(--border-accent);
}

.hex-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 10px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  overflow-x: auto;
}

.tool-group {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.tool-label {
  font-size: 10px;
  color: var(--text-muted);
}
.tool-input {
  width: 90px;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 2px 6px;
}
.tool-input:focus {
  outline: none;
  border-color: var(--border-focus);
}
.tool-input-addr {
  width: 78px;
}
.tool-input-fill {
  width: 96px;
}

.hex-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
}

.hex-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 200px;
  color: var(--text-muted);
}

.hex-row {
  display: flex;
  align-items: center;
  padding: 0 10px;
  transition: background 60ms;
}
.hex-row:hover {
  background: var(--accent-subtle);
}

.addr-text {
  text-align: right;
  color: var(--text-secondary);
  font-size: 11px;
  flex-shrink: 0;
}

.byte-cell {
  width: 22px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  color: var(--text-primary);
  cursor: crosshair;
  border-radius: 2px;
}
.byte-cell:hover {
  background: var(--bg-overlay);
}
.byte-null {
  color: var(--text-muted);
}
.byte-ff {
  color: var(--color-warn);
}
.byte-pad {
  color: transparent;
  cursor: default;
}
.byte-hit {
  background: var(--info-soft);
}
.byte-editing {
  background: var(--bg-overlay);
}

.byte-edit {
  width: 100%;
  height: 100%;
  border: 1px solid var(--border-focus);
  border-radius: 2px;
  background: var(--bg-base);
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 11px;
  text-align: center;
  padding: 0;
}
.byte-edit:focus {
  outline: none;
}

.ascii-text {
  padding-left: 12px;
  border-left: 1px solid var(--border);
  color: var(--accent-dim);
  letter-spacing: 0;
  white-space: pre;
  overflow: hidden;
  flex-shrink: 0;
}

.hex-footer {
  display: flex;
  gap: 16px;
  padding: 5px 12px;
  background: var(--bg-elevated);
  border-top: 1px solid var(--border);
  font-size: 10px;
  color: var(--text-muted);
  flex-shrink: 0;
}
</style>
