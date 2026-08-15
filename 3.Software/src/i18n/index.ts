import { ref } from 'vue'

export type Locale = 'zh' | 'en'

export const locale = ref<Locale>('zh')

export function setLocale(l: Locale) {
  locale.value = l
}

type MessageRecord = Record<string, string>

const zh: MessageRecord = {
  // App / titlebar
  'app.title': 'UnProg',
  'app.minimize': '最小化',
  'app.maximize': '最大化',
  'app.close': '关闭',
  'app.switchLocale': '切换到英文',
  'app.dragToResize': '拖拽调整大小',

  // Pane titles
  'pane.hexView': 'Hex View',
  'pane.outputLog': 'Output Log',

  // Actions
  'action.clearLog': '清空日志',
  'action.connect': '连接',
  'action.reconnect': '重新连接',
  'action.openFile': '打开文件...',
  'action.detect': '检测',
  'action.search': '查找',
  'action.read': '读取',
  'action.write': '写入',
  'action.erase': '擦除',
  'action.verify': '校验',
  'action.cancel': '取消',
  'action.confirmErase': '确认擦除',
  'action.saveBin': '保存 BIN',
  'action.saveHex': '保存 HEX',

  // Sections
  'section.programmer': '编程器',
  'section.file': '文件',
  'section.chip': '芯片',
  'section.operations': '操作',

  // Field labels
  'label.type': '类型',
  'label.serialPort': '串口号',
  'label.vcc18Adapter': '1.8V 适配器',
  'label.spiMode': 'SPI 模式',
  'label.spiClock': 'SPI 时钟',
  'label.vendor': '厂商',
  'label.model': '芯片型号',

  // Placeholders
  'placeholder.serialPort': '例如 COM3',
  'placeholder.selectType': '-- 选择类型 --',
  'placeholder.selectVendor': '-- 选择厂商 --',
  'placeholder.selectModel': '-- 选择型号 --',

  // Options
  'option.serprog': 'Serprog (串口)',
  'option.hidprog': 'HIDProg (预留)',

  // Chip info labels
  'chipInfo.jedec': 'JEDEC:',
  'chipInfo.capacity': '容量:',
  'chipInfo.page': '页:',
  'chipInfo.sector': '扇区:',
  'chipInfo.block': '块:',
  'chipInfo.vcc': 'VCC:',
  'chipInfo.addr4': '4B 地址:',

  // Common
  'common.yes': '是',
  'common.no': '否',

  // Erase confirm modal
  'modal.eraseTitle': '确认全片擦除',
  'modal.eraseBody': '此操作将不可逆地擦除整个芯片，所有数据将变为 FF。\n请确保已备份重要数据。',

  // Log console
  'log.autoScroll': '自动滚动',
  'log.waiting': '等待输出...',
  'log.lines': '行',

  // Status bar
  'status.programmerFail': '编程器失败',
  'status.chipRuleFail': '芯片规则失败',
  'status.connected': '已连接',
  'status.disconnected': '未连接',
  'status.noFile': '未加载文件',

  // Hex viewer
  'hex.address': '地址',
  'hex.ascii': 'ASCII',
  'hex.noData': '未加载数据',
  'hex.bytes': '字节',
  'hex.kb': 'KB',
  'hex.rows': '行',
  'hex.search': '搜索',
  'hex.goto': '跳转',
  'hex.fill': '填充',
  'hex.checksum': '校验和',
  'hex.undo': '撤销',
  'hex.searchPlaceholder': '如 AA BB 00',
  'hex.gotoPlaceholder': '00000000',
  'hex.fillPlaceholder': '如 FF 或 00 11',
  'hex.notFound': '未找到匹配字节序列',
  'hex.foundAt': '找到 @',
  'hex.fillDone': '已填充缓冲区',
  'hex.editHint': '点击字节可编辑',
}

const en: MessageRecord = {
  // App / titlebar
  'app.title': 'UnProg',
  'app.minimize': 'Minimize',
  'app.maximize': 'Maximize',
  'app.close': 'Close',
  'app.switchLocale': 'Switch to Chinese',
  'app.dragToResize': 'Drag to resize',

  // Pane titles
  'pane.hexView': 'Hex View',
  'pane.outputLog': 'Output Log',

  // Actions
  'action.clearLog': 'Clear log',
  'action.connect': 'Connect',
  'action.reconnect': 'Reconnect',
  'action.openFile': 'Open File...',
  'action.detect': 'Detect',
  'action.search': 'Search',
  'action.read': 'Read',
  'action.write': 'Write',
  'action.erase': 'Erase',
  'action.verify': 'Verify',
  'action.cancel': 'Cancel',
  'action.confirmErase': 'Confirm Erase',
  'action.saveBin': 'Save BIN',
  'action.saveHex': 'Save HEX',

  // Sections
  'section.programmer': 'Programmer',
  'section.file': 'File',
  'section.chip': 'Chip',
  'section.operations': 'Operations',

  // Field labels
  'label.type': 'Type',
  'label.serialPort': 'Serial Port',
  'label.vcc18Adapter': '1.8V Adapter',
  'label.spiMode': 'SPI Mode',
  'label.spiClock': 'SPI Clock',
  'label.vendor': 'Vendor',
  'label.model': 'Chip Model',

  // Placeholders
  'placeholder.serialPort': 'e.g. COM3',
  'placeholder.selectType': '-- Select Type --',
  'placeholder.selectVendor': '-- Select Vendor --',
  'placeholder.selectModel': '-- Select Model --',

  // Options
  'option.serprog': 'Serprog (Serial)',
  'option.hidprog': 'HIDProg (Reserved)',

  // Chip info labels
  'chipInfo.jedec': 'JEDEC:',
  'chipInfo.capacity': 'Size:',
  'chipInfo.page': 'Page:',
  'chipInfo.sector': 'Sector:',
  'chipInfo.block': 'Block:',
  'chipInfo.vcc': 'VCC:',
  'chipInfo.addr4': '4B Address:',

  // Common
  'common.yes': 'Yes',
  'common.no': 'No',

  // Erase confirm modal
  'modal.eraseTitle': 'Confirm Full Chip Erase',
  'modal.eraseBody': 'This will irreversibly erase the entire chip. All data will become FF.\nPlease make sure you have backed up important data.',

  // Log console
  'log.autoScroll': 'Auto-scroll',
  'log.waiting': 'Waiting for output...',
  'log.lines': 'lines',

  // Status bar
  'status.programmerFail': 'Programmer Fail',
  'status.chipRuleFail': 'Chip Rule Fail',
  'status.connected': 'Connected',
  'status.disconnected': 'Disconnected',
  'status.noFile': 'No file loaded',

  // Hex viewer
  'hex.address': 'Address',
  'hex.ascii': 'ASCII',
  'hex.noData': 'No data loaded',
  'hex.bytes': 'bytes',
  'hex.kb': 'KB',
  'hex.rows': 'rows',
  'hex.search': 'Search',
  'hex.goto': 'Goto',
  'hex.fill': 'Fill',
  'hex.checksum': 'Checksum',
  'hex.undo': 'Undo',
  'hex.searchPlaceholder': 'e.g. AA BB 00',
  'hex.gotoPlaceholder': '00000000',
  'hex.fillPlaceholder': 'e.g. FF or 00 11',
  'hex.notFound': 'Byte sequence not found',
  'hex.foundAt': 'Found @',
  'hex.fillDone': 'Buffer filled',
  'hex.editHint': 'Click a byte to edit',
}

export const messages: Record<Locale, MessageRecord> = { zh, en }

export function t(key: string): string {
  return messages[locale.value][key] ?? messages.en[key] ?? key
}
