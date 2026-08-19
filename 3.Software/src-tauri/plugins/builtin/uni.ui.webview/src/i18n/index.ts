import { ref } from 'vue'

export type Locale = 'zh' | 'en'

export const locale = ref<Locale>('zh')

export function setLocale(l: Locale) {
  locale.value = l
}

type MessageRecord = Record<string, string>

const zh: MessageRecord = {
  // App / titlebar
  'app.title': 'UniProg',
  'app.minimize': '最小化',
  'app.maximize': '最大化',
  'app.close': '关闭',
  'app.settings': '设置',
  'app.about': '关于',
  'app.dragToResize': '拖拽调整大小',
  'app.closeDuringRunTitle': '正在执行操作',
  'app.closeDuringRunBody': '正在执行操作，关闭可能会带来不可预知的后果，是否确认关闭？',
  'app.confirmClose': '确认',

  // Pane titles
  'pane.hexView': 'Hex View',
  'pane.outputLog': 'Output Log',

  // Actions
  'action.clearLog': '清空日志',
  'action.connect': '连接',
  'action.reconnect': '重新连接',
  'action.rescan': '重新扫描',
  'action.openFile': '打开文件...',
  'action.detect': '检测',
  'action.search': '查找',
  'action.read': '读取',
  'action.write': '写入',
  'action.erase': '擦除',
  'action.verify': '校验',
  'action.blankCheck': '查空',
  'action.auto': '自动',
  'action.cancel': '取消',
  'action.confirmErase': '确认擦除',
  'action.confirm': '确定',
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
  'label.detectedProgrammer': '已检测到',
  'label.serialPort': '串口号',
  'label.spiMode': 'SPI 模式',
  'label.spiClock': 'SPI 时钟',
  'label.vendor': '厂商',
  'label.model': '芯片型号',

  // Placeholders
  'placeholder.serialPort': '例如 COM3',
  'placeholder.selectType': '-- 选择类型 --',
  'placeholder.selectVendor': '-- 选择厂商 --',
  'placeholder.selectModel': '-- 选择型号 --',
  'placeholder.noProgrammer': '未检测到编程器，可手动选择连接',

  // Options
  'option.serprog': 'Serprog (串口)',
  'option.hidprog': 'HIDProg (预留)',

  // Sidecar plugins
  'sidecar.title': 'Sidecar 插件（实验性）',
  'sidecar.refresh': '刷新适配器',
  'sidecar.readId': '读取芯片 ID',
  'sidecar.select': '选定为当前编程器',
  'sidecar.unselect': '取消选择',
  'sidecar.readSize': '读取大小（字节）',
  'sidecar.erase': '擦除',
  'sidecar.read': '读取',
  'sidecar.write': '写入',
  'sidecar.verify': '校验',

  // Plugin manager
  'pluginManager.title': '插件管理器',
  'pluginManager.open': '管理插件',
  'pluginManager.installed': '已安装插件',
  'pluginManager.builtin': '内置模块（uni-base）',
  'pluginManager.enable': '启用',
  'pluginManager.disable': '禁用',
  'pluginManager.close': '关闭',
  'pluginManager.runningDisabled': '操作进行中，禁止修改插件',

  // Programmer auto detection
  'programmer.autoMode': '自动识别',
  'programmer.manualMode': '手动选择',
  'programmer.startDetect': '开始识别',
  'programmer.scanning': '识别中...',
  'programmer.autoPoll': '自动模式',
  'programmer.autoPollHint': '自动模式：持续扫描并自动连接检测到的编程器，连接成功后自动停止',

  // Auto operation
  'auto.settings': '自动流程',
  'auto.stepRead': '读取',
  'auto.stepErase': '擦除',
  'auto.stepBlankCheck': '查空',
  'auto.stepWrite': '写入',
  'auto.stepVerify': '校验',
  'auto.emptyHint': '尚未选择任何步骤',
  'auto.allStepsUsed': '五个步骤都已加入，可继续添加重复步骤（用“+”重新选择）',
  'auto.dragHint': '按住此把手拖拽排序',
  'auto.run': '开始执行',
  'auto.close': '关闭',
  'auto.save': '保存',
  'auto.confirmTitle': '确认执行自动流程',
  'auto.confirmBody': '该流程包含擦除/写入操作，将不可逆地修改芯片内容。请确认已备份重要数据。',

  // Chip info labels
  'chipInfo.jedec': 'JEDEC:',
  'chipInfo.capacity': '容量:',
  'chipInfo.page': '页:',
  'chipInfo.sector': '扇区:',
  'chipInfo.block': '块:',
  'chipInfo.vcc': 'VCC:',
  'chipInfo.dummyMode': 'Dummy:',
  'chipInfo.readMode': '读取模式:',
  'chipInfo.writeMode': '写入模式:',
  'chipInfo.feature': '特性位:',
  'chipInfo.addr4': '4B 地址:',
  'chipInfo.none': '尚未选择或检测到芯片',
  'chip.autoDetect': '自动检测芯片',
  'chip.autoDetectCount': '检测次数',
  'chip.autoDetectInterval': '间隔(秒)',

  // Common
  'common.yes': '是',
  'common.no': '否',
  'status.scanningProgrammers': '正在扫描编程器...',

  // Experimental warning
  'experimental.title': '实验性功能',
  'experimental.body': '该功能尚未经过真机验证，可能无法正常工作或造成数据异常。',
  'experimental.continue': '仍然继续',

  // SPI NOR write protection
  'nor.wpCheck': '检查写保护',
  'nor.wpDisable': '解除写保护',

  // SPI NAND settings
  'section.nand': 'NAND 设置',
  'nand.readBadBlockFirst': '读写前默认先读坏块',
  'nand.badBlockMode': '坏块处理模式',
  'nand.mode.skip': '跳过坏块 (Skip)',
  'nand.mode.bypass': '绕过坏块 (Bypass)',
  'nand.mode.ignore': '忽略坏块 (Ignore)',
  'nand.modeExpLog': '坏块处理模式：{0}（实验性功能，需真机验证）',
  'nand.programMode': '编程模式',
  'nand.programModeHint': 'OOB 编程模式后端尚未实现，当前固定为主数据区',
  'nand.prog.main': '操作主数据区',
  'nand.prog.oobAuto': 'OOB 区域数据自动处理',
  'nand.prog.mainOob': '操作主数据区+OOB备份区',
  'nand.scanBadBlocks': '读取坏块',
  'nand.advanced': '高级功能（实验性）',
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

  // Settings / About
  'settings.autoTitle': '通用自动选项',
  'settings.batchBurn': '连续烧录功能',
  'settings.saveVoltage': '保存设置电压',
  'settings.powerAutoDetect': '上电自动检测',
  'settings.autoDetectEeprom': '自动识别 EEPROM',
  'settings.progressEstimate': '进度条估算(速度快)',
  'settings.checkSoundSwitch': '校验成功提示音',
  'settings.blankCheckAfterErase': '擦除后自动查空',
  'settings.vccControl': '开启电压控制（需编程器支持）',
  'settings.vccBusyHint': '正在输出电压，请先断开电源再关闭电压控制',
  'settings.expEnabledLog': '{0}：实验性功能，已保存设置（后端暂未完全实现）',
  'settings.language': '语言',
  'settings.theme': '主题',
  'settings.themeDark': '深色',
  'settings.themeLight': '浅色',
  'settings.themeSystem': '跟随系统',
  'about.placeholder': '关于内容待定',
  'about.subtitle': 'UniProgrammer',
  'about.betaNotice': '本项目处于测试状态，请谨慎使用',
  'about.author': '作者',
  'about.license': '许可证',
  'about.hardware': '支持硬件',
  'about.hardwareValue': 'CH341A / CH347T / CH347F / serprog / HIDProg（预留）',
  'about.chipTypes': '支持芯片',
  'about.chipTypesValue': 'SPI NOR / SPI NAND / SPI EEPROM / I2C / Microwire / DataFlash 45',
  'about.chipLibrary': '芯片库',
  'about.total': '总计',
  'about.openGitHub': '打开 GitHub',
  'about.protocol.SPI_EC': 'SPI EC',
  'about.protocol.SPI_DATA_45': 'DataFlash 45',
  'about.protocol.SPI_NAND': 'SPI NAND Flash',
  'about.protocol.SPI_NOR': 'SPI NOR Flash',
  'about.protocol.SPI_EEPROM': 'SPI EEPROM',
  'about.protocol.SPI_F-RAM': 'SPI F-RAM',
  'about.protocol.I2C': 'I2C EEPROM',
  'about.protocol.I2C_F-RAM': 'I2C F-RAM',
  'about.protocol.I2C_SPD': 'I2C SPD',
  'about.protocol.Microwire': 'Microwire EEPROM',
  'about.protocol.AVR': 'AVR',
  'about.protocol.MCU': 'MCU',
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
  'vcc.modalTitle': '确认接通电源？',
  'vcc.modalBodyVoltage':
    '即将以 {0} V 作为目标电压接通电源。\n输出电压可能损坏目标芯片、编程器或电脑 USB 端口，请先确认目标芯片的供电电压。',
  'vcc.masterTitle': '开启电压控制？',
  'vcc.masterBody':
    '开启后侧栏将显示“电压调节”区块，允许选择目标电压与接通电源。\n请确认编程器支持电压控制，并始终先核对目标芯片供电电压。',
  'vcc.masterEnable': '开启',
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
}

const en: MessageRecord = {
  // App / titlebar
  'app.title': 'UniProg',
  'app.minimize': 'Minimize',
  'app.maximize': 'Maximize',
  'app.close': 'Close',
  'app.settings': 'Settings',
  'app.about': 'About',
  'app.dragToResize': 'Drag to resize',
  'app.closeDuringRunTitle': 'Operation In Progress',
  'app.closeDuringRunBody':
    'An operation is in progress. Closing now may cause unpredictable results. Close anyway?',
  'app.confirmClose': 'Confirm',

  // Pane titles
  'pane.hexView': 'Hex View',
  'pane.outputLog': 'Output Log',

  // Actions
  'action.clearLog': 'Clear log',
  'action.connect': 'Connect',
  'action.reconnect': 'Reconnect',
  'action.rescan': 'Rescan',
  'action.openFile': 'Open File...',
  'action.detect': 'Detect',
  'action.search': 'Search',
  'action.read': 'Read',
  'action.write': 'Write',
  'action.erase': 'Erase',
  'action.verify': 'Verify',
  'action.blankCheck': 'Blank Check',
  'action.auto': 'Auto',
  'action.cancel': 'Cancel',
  'action.confirmErase': 'Confirm Erase',
  'action.confirm': 'OK',
  'action.saveBin': 'Save BIN',
  'action.saveHex': 'Save HEX',

  // Sections
  'section.programmer': 'Programmer',
  'section.file': 'File',
  'section.chip': 'Chip',
  'section.chipInfo': 'Chip Information',
  'section.operations': 'Operations',

  // SPI NOR write protection
  'nor.wpCheck': 'Check Write Protect',
  'nor.wpDisable': 'Disable Write Protect',

  // Field labels
  'label.type': 'Type',
  'label.detectedProgrammer': 'Detected',
  'label.serialPort': 'Serial Port',
  'label.spiMode': 'SPI Mode',
  'label.spiClock': 'SPI Clock',
  'label.vendor': 'Vendor',
  'label.model': 'Chip Model',

  // Placeholders
  'placeholder.serialPort': 'e.g. COM3',
  'placeholder.selectType': '-- Select Type --',
  'placeholder.selectVendor': '-- Select Vendor --',
  'placeholder.selectModel': '-- Select Model --',
  'placeholder.noProgrammer': 'No programmer detected; use manual connect below',

  // Options
  'option.serprog': 'Serprog (Serial)',
  'option.hidprog': 'HIDProg (Reserved)',

  // Sidecar plugins
  'sidecar.title': 'Sidecar Plugins (Experimental)',
  'sidecar.refresh': 'Refresh Adapters',
  'sidecar.readId': 'Read Chip ID',
  'sidecar.select': 'Select as Current Programmer',
  'sidecar.unselect': 'Deselect',
  'sidecar.readSize': 'Read Size (bytes)',
  'sidecar.erase': 'Erase',
  'sidecar.read': 'Read',
  'sidecar.write': 'Write',
  'sidecar.verify': 'Verify',

  // Plugin manager
  'pluginManager.title': 'Plugin Manager',
  'pluginManager.open': 'Manage Plugins',
  'pluginManager.installed': 'Installed Plugins',
  'pluginManager.builtin': 'Built-in Modules (uni-base)',
  'pluginManager.enable': 'Enable',
  'pluginManager.disable': 'Disable',
  'pluginManager.close': 'Close',
  'pluginManager.runningDisabled': 'Operation in progress; plugin changes disabled',

  // Programmer auto detection
  'programmer.autoMode': 'Auto Detect',
  'programmer.manualMode': 'Manual',
  'programmer.startDetect': 'Start Scan',
  'programmer.scanning': 'Scanning...',
  'programmer.autoPoll': 'Auto Mode',
  'programmer.autoPollHint':
    'Auto mode: keep scanning, connect detected programmers automatically and stop once connected',

  // Auto operation
  'auto.settings': 'Auto Flow',
  'auto.stepRead': 'Read',
  'auto.stepErase': 'Erase',
  'auto.stepBlankCheck': 'Blank Check',
  'auto.stepWrite': 'Write',
  'auto.stepVerify': 'Verify',
  'auto.emptyHint': 'No steps selected yet',
  'auto.allStepsUsed': 'All five steps are in the flow; use + to add duplicate steps',
  'auto.dragHint': 'Drag by this handle to reorder',
  'auto.run': 'Run',
  'auto.close': 'Close',
  'auto.save': 'Save',
  'auto.confirmTitle': 'Confirm Auto Flow',
  'auto.confirmBody':
    'This flow includes erase/write operations and will irreversibly modify the chip. Make sure important data is backed up.',

  // Chip info labels
  'chipInfo.jedec': 'JEDEC:',
  'chipInfo.capacity': 'Size:',
  'chipInfo.page': 'Page:',
  'chipInfo.sector': 'Sector:',
  'chipInfo.block': 'Block:',
  'chipInfo.vcc': 'VCC:',
  'chipInfo.dummyMode': 'Dummy:',
  'chipInfo.readMode': 'Read Mode:',
  'chipInfo.writeMode': 'Write Mode:',
  'chipInfo.feature': 'Feature:',
  'chipInfo.addr4': '4B Address:',
  'chipInfo.none': 'No chip selected or detected yet',
  'chip.autoDetect': 'Auto detect chip',
  'chip.autoDetectCount': 'Attempts',
  'chip.autoDetectInterval': 'Interval (s)',

  // Common
  'common.yes': 'Yes',
  'common.no': 'No',
  'status.scanningProgrammers': 'Scanning for programmers...',

  // Experimental warning
  'experimental.title': 'Experimental Feature',
  'experimental.body':
    'This feature has not been validated on real hardware and may malfunction or corrupt data.',
  'experimental.continue': 'Continue Anyway',

  // SPI NAND settings
  'section.nand': 'NAND Settings',
  'nand.readBadBlockFirst': 'Scan bad blocks before read/write',
  'nand.badBlockMode': 'Bad Block Mode',
  'nand.mode.skip': 'Skip',
  'nand.mode.bypass': 'Bypass (BBM)',
  'nand.mode.ignore': 'Ignore',
  'nand.modeExpLog': 'Bad block mode: {0} (experimental, requires hardware validation)',
  'nand.programMode': 'Program Mode',
  'nand.programModeHint': 'OOB program modes are not implemented yet; main data area is used',
  'nand.prog.main': 'Main data area only',
  'nand.prog.oobAuto': 'OOB area handled automatically',
  'nand.prog.mainOob': 'Main + OOB backup area',
  'nand.scanBadBlocks': 'Scan Bad Blocks',
  'nand.advanced': 'Advanced (experimental)',
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

  // Settings / About
  'settings.autoTitle': 'General Auto Options',
  'settings.batchBurn': 'Continuous Programming',
  'settings.saveVoltage': 'Save Voltage Setting',
  'settings.powerAutoDetect': 'Auto Detect on Power-up',
  'settings.autoDetectEeprom': 'Auto Detect EEPROM',
  'settings.progressEstimate': 'Fast Progress Estimation',
  'settings.checkSoundSwitch': 'Verify Success Sound',
  'settings.blankCheckAfterErase': 'Blank check after erase',
  'settings.vccControl': 'Enable Voltage Control (programmer support required)',
  'settings.vccBusyHint': 'Power is on. Disconnect power before disabling voltage control.',
  'settings.expEnabledLog': '{0}: experimental setting saved (backend not fully implemented)',
  'settings.language': 'Language',
  'settings.theme': 'Theme',
  'settings.themeDark': 'Dark',
  'settings.themeLight': 'Light',
  'settings.themeSystem': 'Follow System',
  'about.placeholder': 'About content to be determined',
  'about.subtitle': 'UniProgrammer',
  'about.betaNotice': 'This project is in testing status, use with caution',
  'about.author': 'Author',
  'about.license': 'License',
  'about.hardware': 'Hardware',
  'about.hardwareValue': 'CH341A / CH347T / CH347F / serprog / HIDProg (reserved)',
  'about.chipTypes': 'Chip Types',
  'about.chipTypesValue': 'SPI NOR / SPI NAND / SPI EEPROM / I2C / Microwire / DataFlash 45',
  'about.chipLibrary': 'Chip Database',
  'about.total': 'Total',
  'about.openGitHub': 'Open GitHub',
  'about.protocol.SPI_EC': 'SPI EC',
  'about.protocol.SPI_DATA_45': 'DataFlash 45',
  'about.protocol.SPI_NAND': 'SPI NAND Flash',
  'about.protocol.SPI_NOR': 'SPI NOR Flash',
  'about.protocol.SPI_EEPROM': 'SPI EEPROM',
  'about.protocol.SPI_F-RAM': 'SPI F-RAM',
  'about.protocol.I2C': 'I2C EEPROM',
  'about.protocol.I2C_F-RAM': 'I2C F-RAM',
  'about.protocol.I2C_SPD': 'I2C SPD',
  'about.protocol.Microwire': 'Microwire EEPROM',
  'about.protocol.AVR': 'AVR',
  'about.protocol.MCU': 'MCU',
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
  'vcc.modalTitle': 'Enable Power?',
  'vcc.modalBodyVoltage':
    'Power will be enabled at {0} V.\nThis may damage the target chip, the programmer, or the computer USB port. Verify the target chip supply voltage first.',
  'vcc.masterTitle': 'Enable Voltage Control?',
  'vcc.masterBody':
    'The Voltage Regulation section will appear in the sidebar.\nMake sure your programmer supports voltage control and always verify the target chip supply voltage.',
  'vcc.masterEnable': 'Enable',
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
}

export const messages: Record<Locale, MessageRecord> = { zh, en }

export function t(key: string): string {
  return messages[locale.value][key] ?? messages.en[key] ?? key
}
