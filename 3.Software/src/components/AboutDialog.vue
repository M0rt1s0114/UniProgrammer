<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { call } from '@/services/ipc'
import { getVersion } from '@tauri-apps/api/app'
import { t } from '@/i18n'

const open = defineModel<boolean>('open', { default: false })

const REPO_URL = 'https://github.com/M0rt1s0114/UniProgrammer'
const version = ref('')
const total = ref(0)
const stats = ref<{ protocol: string; count: number }[]>([])

const protocolOrder = [
  'SPI_NOR',
  'SPI_NAND',
  'SPI_EEPROM',
  'SPI_F-RAM',
  'I2C',
  'I2C_F-RAM',
  'I2C_SPD',
  'Microwire',
  'SPI_DATA_45',
  'MCU',
  'AVR',
  'SPI_EC',
]

function protocolLabel(protocol: string): string {
  return t(`about.protocol.${protocol}`)
}

async function loadAbout() {
  try {
    await call('load_chip_lib')
    const result = (await call('get_chip_lib_stats')) as {
      total: number
      counts: { protocol: string; count: number }[]
    }
    total.value = result.total
    const byProtocol = new Map(result.counts.map((item) => [item.protocol, item.count]))
    stats.value = protocolOrder
      .filter((protocol) => (byProtocol.get(protocol) ?? 0) > 0)
      .map((protocol) => ({ protocol, count: byProtocol.get(protocol) ?? 0 }))
  } catch (e: unknown) {
    console.warn('load about info failed:', e)
  }
  try {
    version.value = await getVersion()
  } catch {
    version.value = ''
  }
}

async function openGitHub() {
  try {
    await call('open_external_url', { url: REPO_URL })
  } catch (e: unknown) {
    console.warn('open github failed:', e)
  }
}

function close() {
  open.value = false
}

onMounted(loadAbout)
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="modal-backdrop" @click.self="close">
      <div class="modal about-modal">
        <div class="about-head">
          <div class="about-icon">
            <svg width="30" height="30" viewBox="0 0 640 640" fill="currentColor">
              <path
                d="M240 88C240 74.7 229.3 64 216 64C202.7 64 192 74.7 192 88L192 128C156.7 128 128 156.7 128 192L88 192C74.7 192 64 202.7 64 216C64 229.3 74.7 240 88 240L128 240L128 296L88 296C74.7 296 64 306.7 64 320C64 333.3 74.7 344 88 344L128 344L128 400L88 400C74.7 400 64 410.7 64 424C64 437.3 74.7 448 88 448L128 448C128 483.3 156.7 512 192 512L192 552C192 565.3 202.7 576 216 576C229.3 576 240 565.3 240 552L240 512L296 512L296 552C296 565.3 306.7 576 320 576C333.3 576 344 565.3 344 552L344 512L400 512L400 552C400 565.3 410.7 576 424 576C437.3 576 448 565.3 448 552L448 512C483.3 512 512 483.3 512 448L552 448C565.3 448 576 437.3 576 424C576 410.7 565.3 400 552 400L512 400L512 344L552 344C565.3 344 576 333.3 576 320C576 306.7 565.3 296 552 296L512 296L512 240L552 240C565.3 240 576 229.3 576 216C576 202.7 565.3 192 552 192L512 192C512 156.7 483.3 128 448 128L448 88C448 74.7 437.3 64 424 64C410.7 64 400 74.7 400 88L400 128L344 128L344 88C344 74.7 333.3 64 320 64C306.7 64 296 74.7 296 88L296 128L240 128L240 88zM224 192L416 192C433.7 192 448 206.3 448 224L448 416C448 433.7 433.7 448 416 448L224 448C206.3 448 192 433.7 192 416L192 224C192 206.3 206.3 192 224 192zM240 240L240 400L400 400L400 240L240 240z"
              />
            </svg>
          </div>
          <h3 class="about-name">Universal Programmer Tool</h3>
          <div class="about-sub">{{ t('about.subtitle') }}</div>
          <div v-if="version" class="about-version">v{{ version }}</div>
          <div class="about-beta-notice">{{ t('about.betaNotice') }}</div>
        </div>

        <div class="about-divider" />

        <div class="about-rows">
          <div class="about-row">
            <span class="about-row-label">{{ t('about.author') }}</span>
            <span class="about-row-value">M0rt1s0114</span>
          </div>
          <div class="about-row">
            <span class="about-row-label">{{ t('about.license') }}</span>
            <span class="about-row-value">GPL-3.0-or-later</span>
          </div>
          <div class="about-row">
            <span class="about-row-label">{{ t('about.hardware') }}</span>
            <span class="about-row-value">{{ t('about.hardwareValue') }}</span>
          </div>
          <div class="about-row">
            <span class="about-row-label">{{ t('about.chipTypes') }}</span>
            <span class="about-row-value">{{ t('about.chipTypesValue') }}</span>
          </div>
        </div>

        <div class="about-divider" />

        <div class="about-chip-library">
          <div class="about-chip-title">
            {{ t('about.chipLibrary') }}
            <span class="about-chip-total">· {{ t('about.total') }} {{ total }}</span>
          </div>
          <div class="about-chip-grid">
            <div v-for="item in stats" :key="item.protocol" class="about-chip-row">
              <span class="about-chip-name">{{ protocolLabel(item.protocol) }}</span>
              <span class="about-chip-count">{{ item.count }}</span>
            </div>
          </div>
        </div>

        <div class="about-repo">
          <button class="about-repo-link" @click="openGitHub">{{ REPO_URL }}</button>
        </div>

        <div class="modal-actions">
          <button class="btn btn-secondary" @click="openGitHub">
            {{ t('about.openGitHub') }}
          </button>
          <button class="btn btn-primary" @click="close">{{ t('action.confirm') }}</button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.about-modal {
  max-width: 470px;
  text-align: center;
  gap: 12px;
}
.about-head {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}
.about-icon {
  color: var(--accent);
  margin-bottom: 2px;
}
.about-name {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}
.about-sub {
  font-size: 11px;
  color: var(--text-muted);
  letter-spacing: 0.04em;
}
.about-version {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
}
.about-beta-notice {
  margin-top: 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--color-warn);
}
.about-divider {
  height: 1px;
  background: var(--border);
}
.about-rows {
  display: flex;
  flex-direction: column;
  gap: 7px;
  text-align: left;
}
.about-row {
  display: flex;
  gap: 10px;
  font-size: 12px;
  line-height: 1.5;
}
.about-row-label {
  flex-shrink: 0;
  width: 72px;
  color: var(--text-muted);
}
.about-row-value {
  color: var(--text-secondary);
}
.about-chip-library {
  text-align: left;
}
.about-chip-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}
.about-chip-total {
  font-family: var(--font-mono);
  font-weight: 500;
  color: var(--text-secondary);
}
.about-chip-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 16px;
}
.about-chip-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  padding: 3px 8px;
  border-radius: var(--radius-sm);
  background: var(--bg-base);
}
.about-chip-name {
  color: var(--text-secondary);
}
.about-chip-count {
  font-family: var(--font-mono);
  color: var(--text-accent);
}
.about-repo {
  display: flex;
  justify-content: center;
}
.about-repo-link {
  border: none;
  background: transparent;
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 11px;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 3px;
}
.about-repo-link:hover {
  color: var(--text-accent);
}
</style>
