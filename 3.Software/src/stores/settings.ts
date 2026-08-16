import { ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { setLocale, type Locale } from '@/i18n'

export type ThemeMode = 'dark' | 'light' | 'system'

export const VCC_LEVELS_MV = [1200, 1800, 2500, 3300] as const

export const DEFAULT_SETTINGS = {
  language: 'zh' as Locale,
  theme: 'dark' as ThemeMode,
  batchBurn: false,
  saveVoltage: false,
  powerAutoDetect: false,
  autoDetectEeprom: false,
  progressEstimate: false,
  checkSoundSwitch: true,
  nandReadBadBlockFirst: true,
  nandBadBlockMode: 'skip',
  nandProgramMode: 'main',
  vccControlEnabled: false,
  vccTargetMv: 3300,
}

const SECTION_GENERAL = 'general'
const SECTION_AUTO = 'auto'
const SECTION_NAND = 'nand'
const SECTION_VCC = 'vcc'

function parseBool(value: string | undefined): boolean | undefined {
  if (value === undefined) return undefined
  const normalized = value.trim().toLowerCase()
  if (['1', 'true', 'yes', 'on'].includes(normalized)) return true
  if (['0', 'false', 'no', 'off'].includes(normalized)) return false
  return undefined
}

function parseNumber(value: string | undefined): number | undefined {
  if (value === undefined) return undefined
  const n = Number(value)
  return Number.isFinite(n) ? n : undefined
}

interface IniSection {
  [key: string]: string
}

function parseIni(text: string): Record<string, IniSection> {
  const result: Record<string, IniSection> = {}
  let section = ''

  for (const rawLine of text.replace(/^\uFEFF/, '').split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line || line.startsWith('#') || line.startsWith(';')) continue

    const sectionMatch = /^\[([^\]]+)\]$/.exec(line)
    if (sectionMatch) {
      section = sectionMatch[1].trim().toLowerCase()
      result[section] ??= {}
      continue
    }

    const eq = line.indexOf('=')
    if (eq <= 0 || !section) continue
    const key = line.slice(0, eq).trim().toLowerCase()
    const value = line.slice(eq + 1).trim()
    result[section] ??= {}
    result[section][key] = value
  }

  return result
}

function validVccMv(value: number | undefined): number {
  if (value !== undefined && (VCC_LEVELS_MV as readonly number[]).includes(value)) {
    return value
  }
  return DEFAULT_SETTINGS.vccTargetMv
}

type PartialSettings = Partial<typeof DEFAULT_SETTINGS>

function parseSettingsFile(text: string): PartialSettings {
  const ini = parseIni(text)
  const general = ini[SECTION_GENERAL] ?? {}
  const auto = ini[SECTION_AUTO] ?? {}
  const nand = ini[SECTION_NAND] ?? {}
  const vcc = ini[SECTION_VCC] ?? {}

  const language = general['language']
  const theme = general['theme']

  const parsed: PartialSettings = {}
  if (language === 'zh' || language === 'en') parsed.language = language
  if (theme === 'dark' || theme === 'light' || theme === 'system') parsed.theme = theme

  for (const [key, field] of [
    ['batchburn', 'batchBurn'],
    ['savevoltage', 'saveVoltage'],
    ['powerautodetect', 'powerAutoDetect'],
    ['autodetecteeprom', 'autoDetectEeprom'],
    ['progressestimate', 'progressEstimate'],
    ['checksoundswitch', 'checkSoundSwitch'],
  ] as const) {
    const value = parseBool(auto[key])
    if (value !== undefined) parsed[field] = value
  }

  const readBadBlockFirst = parseBool(nand['readbadblockfirst'])
  if (readBadBlockFirst !== undefined) parsed.nandReadBadBlockFirst = readBadBlockFirst
  if (['skip', 'bypass', 'ignore'].includes(nand['badblockmode'] ?? '')) {
    parsed.nandBadBlockMode = nand['badblockmode'] as 'skip' | 'bypass' | 'ignore'
  }
  if (['main', 'oob_auto', 'main_oob'].includes(nand['programmode'] ?? '')) {
    parsed.nandProgramMode = nand['programmode'] as 'main' | 'oob_auto' | 'main_oob'
  }

  const controlEnabled = parseBool(vcc['controlenabled'])
  if (controlEnabled !== undefined) parsed.vccControlEnabled = controlEnabled
  // 仅当“保存设置电压”开启时才跨启动记忆目标电压；文件缺省时保持默认
  if (parseBool(auto['savevoltage']) === true) {
    const targetMv = parseNumber(vcc['targetmv'])
    if (targetMv !== undefined) parsed.vccTargetMv = validVccMv(targetMv)
  }

  return parsed
}

const LEGACY_KEYS = [
  'nand.readBadBlockFirst',
  'nand.badBlockMode',
  'nand.programMode',
  'nand.batchBurn',
  'nand.saveVoltage',
  'nand.powerAutoDetect',
  'nand.autoDetectEeprom',
  'nand.progressEstimate',
  'nand.checkSoundSwitch',
  'vcc.targetMv',
] as const

interface LegacySettings {
  batchBurn?: boolean
  saveVoltage?: boolean
  powerAutoDetect?: boolean
  autoDetectEeprom?: boolean
  progressEstimate?: boolean
  checkSoundSwitch?: boolean
  nandReadBadBlockFirst?: boolean
  nandBadBlockMode?: 'skip' | 'bypass' | 'ignore'
  nandProgramMode?: 'main' | 'oob_auto' | 'main_oob'
  vccTargetMv?: number
}

function readLegacySettings(): LegacySettings {
  const result: LegacySettings = {}
  const getBool = (key: string): boolean | undefined => {
    try {
      const raw = localStorage.getItem(key)
      if (raw === null) return undefined
      return JSON.parse(raw) === true
    } catch {
      return undefined
    }
  }
  const getString = (key: string): string | undefined => {
    try {
      const raw = localStorage.getItem(key)
      if (raw === null) return undefined
      return JSON.parse(raw)
    } catch {
      return undefined
    }
  }

  result.batchBurn = getBool('nand.batchBurn')
  result.saveVoltage = getBool('nand.saveVoltage')
  result.powerAutoDetect = getBool('nand.powerAutoDetect')
  result.autoDetectEeprom = getBool('nand.autoDetectEeprom')
  result.progressEstimate = getBool('nand.progressEstimate')
  result.checkSoundSwitch = getBool('nand.checkSoundSwitch')
  result.nandReadBadBlockFirst = getBool('nand.readBadBlockFirst')

  const badBlockMode = getString('nand.badBlockMode')
  if (badBlockMode === 'skip' || badBlockMode === 'bypass' || badBlockMode === 'ignore') {
    result.nandBadBlockMode = badBlockMode
  }
  const programMode = getString('nand.programMode')
  if (programMode === 'main' || programMode === 'oob_auto' || programMode === 'main_oob') {
    result.nandProgramMode = programMode
  }

  if (result.saveVoltage === true) {
    try {
      const mv = Number(localStorage.getItem('vcc.targetMv'))
      result.vccTargetMv = validVccMv(Number.isFinite(mv) ? mv : undefined)
    } catch {
      // ignore malformed legacy value
    }
  }
  return result
}

function clearLegacySettings() {
  for (const key of LEGACY_KEYS) {
    try {
      localStorage.removeItem(key)
    } catch {
      // WebView 禁用存储时忽略
    }
  }
}

interface SerializedSettings {
  language: string
  theme: string
  batchBurn: boolean
  saveVoltage: boolean
  powerAutoDetect: boolean
  autoDetectEeprom: boolean
  progressEstimate: boolean
  checkSoundSwitch: boolean
  nandReadBadBlockFirst: boolean
  nandBadBlockMode: string
  nandProgramMode: string
  vccControlEnabled: boolean
  vccTargetMv: number
}

function serializeSettings(state: SerializedSettings): string {
  const lines: string[] = []
  lines.push('# UniProgrammer settings')
  lines.push('')
  lines.push('[general]')
  lines.push(`language=${state.language}`)
  lines.push(`theme=${state.theme}`)
  lines.push('')
  lines.push('[auto]')
  lines.push(`batchBurn=${state.batchBurn}`)
  lines.push(`saveVoltage=${state.saveVoltage}`)
  lines.push(`powerAutoDetect=${state.powerAutoDetect}`)
  lines.push(`autoDetectEeprom=${state.autoDetectEeprom}`)
  lines.push(`progressEstimate=${state.progressEstimate}`)
  lines.push(`checkSoundSwitch=${state.checkSoundSwitch}`)
  lines.push('')
  lines.push('[nand]')
  lines.push(`readBadBlockFirst=${state.nandReadBadBlockFirst}`)
  lines.push(`badBlockMode=${state.nandBadBlockMode}`)
  lines.push(`programMode=${state.nandProgramMode}`)
  lines.push('')
  lines.push('[vcc]')
  lines.push(`controlEnabled=${state.vccControlEnabled}`)
  // 保存设置电压未开启时，目标电压不跨启动记忆，始终回到安全默认值
  lines.push(`targetMv=${state.saveVoltage ? state.vccTargetMv : DEFAULT_SETTINGS.vccTargetMv}`)
  lines.push('')
  return lines.join('\n')
}

export const useSettingsStore = defineStore('settings', () => {
  const hydrated = ref(false)
  const settingsFilePath = ref('')
  const lastSaveError = ref('')

  const language = ref<Locale>(DEFAULT_SETTINGS.language)
  const theme = ref<ThemeMode>(DEFAULT_SETTINGS.theme)
  const batchBurn = ref(DEFAULT_SETTINGS.batchBurn)
  const saveVoltage = ref(DEFAULT_SETTINGS.saveVoltage)
  const powerAutoDetect = ref(DEFAULT_SETTINGS.powerAutoDetect)
  const autoDetectEeprom = ref(DEFAULT_SETTINGS.autoDetectEeprom)
  const progressEstimate = ref(DEFAULT_SETTINGS.progressEstimate)
  const checkSoundSwitch = ref(DEFAULT_SETTINGS.checkSoundSwitch)
  const nandReadBadBlockFirst = ref(DEFAULT_SETTINGS.nandReadBadBlockFirst)
  const nandBadBlockMode = ref<'skip' | 'bypass' | 'ignore'>(DEFAULT_SETTINGS.nandBadBlockMode)
  const nandProgramMode = ref<'main' | 'oob_auto' | 'main_oob'>(DEFAULT_SETTINGS.nandProgramMode)
  const vccControlEnabled = ref(DEFAULT_SETTINGS.vccControlEnabled)
  const vccTargetMv = ref(DEFAULT_SETTINGS.vccTargetMv)

  function applyLocale() {
    setLocale(language.value)
  }

  let themeListener: ((event: MediaQueryListEvent) => void) | null = null

  function resolveTheme(): 'dark' | 'light' {
    if (theme.value === 'system') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
    }
    return theme.value
  }

  function applyTheme() {
    const resolved = resolveTheme()
    document.documentElement.dataset.theme = resolved

    if (themeListener) return
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    themeListener = (event) => {
      if (theme.value === 'system') {
        document.documentElement.dataset.theme = event.matches ? 'dark' : 'light'
      }
    }
    media.addEventListener('change', themeListener)
  }

  function applyAll(partial: PartialSettings) {
    if (partial.language) {
      language.value = partial.language
    }
    if (partial.theme) {
      theme.value = partial.theme
    }
    if (partial.batchBurn !== undefined) batchBurn.value = partial.batchBurn
    if (partial.saveVoltage !== undefined) saveVoltage.value = partial.saveVoltage
    if (partial.powerAutoDetect !== undefined) powerAutoDetect.value = partial.powerAutoDetect
    if (partial.autoDetectEeprom !== undefined) autoDetectEeprom.value = partial.autoDetectEeprom
    if (partial.progressEstimate !== undefined) progressEstimate.value = partial.progressEstimate
    if (partial.checkSoundSwitch !== undefined) checkSoundSwitch.value = partial.checkSoundSwitch
    if (partial.nandReadBadBlockFirst !== undefined) {
      nandReadBadBlockFirst.value = partial.nandReadBadBlockFirst
    }
    if (partial.nandBadBlockMode !== undefined) nandBadBlockMode.value = partial.nandBadBlockMode
    if (partial.nandProgramMode !== undefined) nandProgramMode.value = partial.nandProgramMode
    if (partial.vccControlEnabled !== undefined) vccControlEnabled.value = partial.vccControlEnabled
    if (partial.vccTargetMv !== undefined) vccTargetMv.value = partial.vccTargetMv
  }

  async function save() {
    try {
      const path = await invoke<string>('save_settings', {
        content: serializeSettings({
          language: language.value,
          theme: theme.value,
          batchBurn: batchBurn.value,
          saveVoltage: saveVoltage.value,
          powerAutoDetect: powerAutoDetect.value,
          autoDetectEeprom: autoDetectEeprom.value,
          progressEstimate: progressEstimate.value,
          checkSoundSwitch: checkSoundSwitch.value,
          nandReadBadBlockFirst: nandReadBadBlockFirst.value,
          nandBadBlockMode: nandBadBlockMode.value,
          nandProgramMode: nandProgramMode.value,
          vccControlEnabled: vccControlEnabled.value,
          vccTargetMv: vccTargetMv.value,
        }),
      })
      settingsFilePath.value = path
      lastSaveError.value = ''
    } catch (e: unknown) {
      lastSaveError.value = String(e)
      console.warn('save settings failed:', e)
    }
  }

  async function initialize() {
    if (hydrated.value) return

    let fileText = ''
    let legacyFound = false
    try {
      fileText = await invoke<string>('load_settings')
    } catch (e: unknown) {
      lastSaveError.value = String(e)
      console.warn('load settings failed:', e)
    }

    const fromFile = parseSettingsFile(fileText)
    const legacy = readLegacySettings()
    for (const value of Object.values(legacy)) {
      if (value !== undefined) {
        legacyFound = true
        break
      }
    }

    // 文件优先；旧 localStorage 作为缺失项的迁移来源；其余用默认值
    applyAll({
      ...DEFAULT_SETTINGS,
      ...legacy,
      ...fromFile,
    })

    if (
      saveVoltage.value &&
      legacy.vccTargetMv !== undefined &&
      fromFile.vccTargetMv === undefined
    ) {
      vccTargetMv.value = legacy.vccTargetMv
    }

    applyLocale()
    applyTheme()
    hydrated.value = true

    // 首次启动时把迁移结果落盘为 Setting.set，随后清理旧 localStorage
    if (legacyFound) {
      await save()
      clearLegacySettings()
    }
  }

  watch(language, () => {
    if (!hydrated.value) return
    applyLocale()
  })

  watch(theme, () => {
    if (!hydrated.value) return
    applyTheme()
  })

  let saveTimer: ReturnType<typeof setTimeout> | null = null
  watch(
    [
      language,
      theme,
      batchBurn,
      saveVoltage,
      powerAutoDetect,
      autoDetectEeprom,
      progressEstimate,
      checkSoundSwitch,
      nandReadBadBlockFirst,
      nandBadBlockMode,
      nandProgramMode,
      vccControlEnabled,
      vccTargetMv,
    ],
    () => {
      if (!hydrated.value) return
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(() => {
        void save()
      }, 200)
    },
  )

  return {
    hydrated,
    settingsFilePath,
    lastSaveError,
    language,
    theme,
    batchBurn,
    saveVoltage,
    powerAutoDetect,
    autoDetectEeprom,
    progressEstimate,
    checkSoundSwitch,
    nandReadBadBlockFirst,
    nandBadBlockMode,
    nandProgramMode,
    vccControlEnabled,
    vccTargetMv,
    initialize,
    save,
    applyTheme,
  }
})
