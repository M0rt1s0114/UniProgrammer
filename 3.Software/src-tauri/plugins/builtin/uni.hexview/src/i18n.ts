import { ref } from 'vue'

export type Locale = 'zh' | 'en'

export const locale = ref<Locale>('zh')

export function setLocale(next: Locale) {
  locale.value = next
}

type MessageRecord = Record<string, string>

const zh: MessageRecord = {
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

const messages: Record<Locale, MessageRecord> = { zh, en }

export function t(key: string): string {
  return messages[locale.value][key] ?? messages.en[key] ?? key
}
