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
  'section.chipInfo': '芯片信息',
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
  'chipInfo.none': '尚未选择或检测到芯片',

  // Common
  'common.yes': '是',
  'common.no': '否',

  // SPI NAND settings
  'section.nand': 'NAND 设置',
  'nand.readBadBlockFirst': '读写前默认先读坏块',
  'nand.badBlockMode': '坏块处理模式',
  'nand.mode.skip': '跳过坏块 (Skip)',
  'nand.mode.bypass': '绕过坏块 (Bypass)',
  'nand.mode.ignore': '忽略坏块 (Ignore)',
  'nand.programMode': '编程模式',
  'nand.prog.main': '操作主数据区',
  'nand.prog.oobAuto': 'OOB 区域数据自动处理',
  'nand.prog.mainOob': '操作主数据区+OOB备份区',
  'nand.scanBadBlocks': '读取坏块',
  'nand.readUid': '读 UID',
  'nand.readParamPage': '读参数页数据',
  'nand.readBbmLut': '读取坏块映射表',
  'nand.eccEnable': '开启硬件 ECC',
  'nand.eccDisable': '关闭硬件 ECC',
  'nand.experimental': '（实验性）',
  'nand.batchBurn': '连续烧录功能',
  'nand.saveVoltage': '保存设置电压',
  'nand.powerAutoDetect': '上电自动检测',
  'nand.autoDetectEeprom': '自动识别 EEPROM',
  'nand.progressEstimate': '进度条估算(速度快)',
  'nand.checkSoundSwitch': '校验成功提示音',
  'nand.readOtpPage': '读 OTP 页',
  'nand.otpPage': 'OTP 页号',
  'nand.spare': 'OOB 容量',
  'nand.pagesPerBlock': '每块页数',
  'nand.isBmm': '支持 BBM',

  // 45-series DataFlash page mode
  'section.at45': '45 芯片模式',
  'at45.readPageMode': '读45页面模式',
  'at45.readChipMode': '读45芯片模式',
  'at45.setDataFlashPage': 'DataFlash页面',
  'at45.setBinaryPage': '二进制页面',
  'at45.confirmDataFlash': '确认将芯片设置为 DataFlash 页面模式？此功能非必要慎用',
  'at45.confirmBinary': '确认将芯片设置为二进制页面模式？此功能非必要慎用',

  // VCC output
  'section.vcc': '电压调节',
  'label.vccVoltage': 'VCC 电压',
  'vcc.connectPower': '接通电源',
  'vcc.disconnectPower': '断开电源',
  'vcc.followChip': '同步芯片库信息',
  'vcc.followHint': '根据芯片库中的 VCC 信息自动设置电压',
  'vcc.noChipVcc': '芯片库中没有 VCC 电压信息',
  'vcc.followLog': '电压跟随芯片库：{0}V',
  'vcc.offHint': '电压输出已关闭（硬件上电默认不输出电压）',
  'vcc.statusOn': 'VCC ON',
  'vcc.modalTitle': '启用 VCC 输出？',
  'vcc.modalBody': '输出电压可能损坏目标芯片、编程器或电脑 USB 端口。请先确认目标芯片的供电电压。',
  'vcc.typeHint': '请输入“{0}”以确认开启',
  'vcc.enablePhrase': '确认开启',
  'vcc.changeTitle': '更改目标电压',
  'vcc.changeBody': '请输入目标电压数值以确认（例如 1.8）。',
  'vcc.changePhraseHint': '请输入 {0} 以确认',
  'vcc.apply': '应用',
  'vcc.wrongPhrase': '输入不匹配，操作已取消',
  'vcc.testEnabled': '电压控制已启用，电压已选择：{0}V',
  'vcc.testDisabled': '电压控制已关闭',
  'vcc.testChanged': '电压已选择：{1}V（原 {0}V）',
  'vcc.voltageUnit': 'V',

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
  'section.chipInfo': 'Chip Information',
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
  'chipInfo.none': 'No chip selected or detected yet',

  // Common
  'common.yes': 'Yes',
  'common.no': 'No',

  // SPI NAND settings
  'section.nand': 'NAND Settings',
  'nand.readBadBlockFirst': 'Scan bad blocks before read/write',
  'nand.badBlockMode': 'Bad Block Mode',
  'nand.mode.skip': 'Skip',
  'nand.mode.bypass': 'Bypass (BBM)',
  'nand.mode.ignore': 'Ignore',
  'nand.programMode': 'Program Mode',
  'nand.prog.main': 'Main data area only',
  'nand.prog.oobAuto': 'OOB area handled automatically',
  'nand.prog.mainOob': 'Main + OOB backup area',
  'nand.scanBadBlocks': 'Scan Bad Blocks',
  'nand.readUid': 'Read UID',
  'nand.readParamPage': 'Read Parameter Page',
  'nand.readBbmLut': 'Read BBM LUT',
  'nand.eccEnable': 'Enable On-Die ECC',
  'nand.eccDisable': 'Disable On-Die ECC',
  'nand.experimental': '(experimental)',
  'nand.batchBurn': 'Continuous Programming',
  'nand.saveVoltage': 'Save Voltage Setting',
  'nand.powerAutoDetect': 'Auto Detect on Power-up',
  'nand.autoDetectEeprom': 'Auto Detect EEPROM',
  'nand.progressEstimate': 'Fast Progress Estimation',
  'nand.checkSoundSwitch': 'Verify Success Sound',
  'nand.readOtpPage': 'Read OTP Page',
  'nand.otpPage': 'OTP Page',
  'nand.spare': 'OOB Size',
  'nand.pagesPerBlock': 'Pages Per Block',
  'nand.isBmm': 'Supports BBM',

  // 45-series DataFlash page mode
  'section.at45': '45 Chip Mode',
  'at45.readPageMode': 'Read Page Mode',
  'at45.readChipMode': 'Read Chip Mode',
  'at45.setDataFlashPage': 'DataFlash Page',
  'at45.setBinaryPage': 'Binary Page',
  'at45.confirmDataFlash': 'Configure the chip for DataFlash page mode? Use with caution.',
  'at45.confirmBinary': 'Configure the chip for binary page mode? Use with caution.',

  // VCC output
  'section.vcc': 'Voltage Regulation',
  'label.vccVoltage': 'VCC Voltage',
  'vcc.connectPower': 'Enable Power',
  'vcc.disconnectPower': 'Disconnect Power',
  'vcc.followChip': 'Follow chip database',
  'vcc.followHint': 'Set voltage automatically from the chip database VCC field',
  'vcc.noChipVcc': 'No VCC information in the chip database',
  'vcc.followLog': 'Voltage follows chip database: {0}V',
  'vcc.offHint': 'Voltage output disabled (hardware defaults to no output at power-up)',
  'vcc.statusOn': 'VCC ON',
  'vcc.modalTitle': 'Enable VCC Output?',
  'vcc.modalBody':
    'Enabling voltage output may damage the target chip, the programmer, or the computer USB port. Verify the target chip supply voltage first.',
  'vcc.typeHint': 'Type "{0}" to enable',
  'vcc.enablePhrase': 'Enable',
  'vcc.changeTitle': 'Change Target Voltage',
  'vcc.changeBody': 'Type the target voltage value to confirm (for example 1.8).',
  'vcc.changePhraseHint': 'Type {0} to confirm',
  'vcc.apply': 'Apply',
  'vcc.wrongPhrase': 'Input does not match, operation cancelled',
  'vcc.testEnabled': 'Voltage control enabled, voltage selected: {0}V',
  'vcc.testDisabled': 'Voltage control disabled',
  'vcc.testChanged': 'Voltage selected: {1}V (was {0}V)',
  'vcc.voltageUnit': 'V',

  // Erase confirm modal
  'modal.eraseTitle': 'Confirm Full Chip Erase',
  'modal.eraseBody':
    'This will irreversibly erase the entire chip. All data will become FF.\nPlease make sure you have backed up important data.',

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
