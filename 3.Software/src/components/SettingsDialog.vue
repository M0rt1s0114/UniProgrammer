<script setup lang="ts">
import { useSettingsStore } from '@/stores/settings'
import { t } from '@/i18n'

const settings = useSettingsStore()
const open = defineModel<boolean>('open', { default: false })

function close() {
  open.value = false
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="modal-backdrop" @click.self="close">
      <div class="modal settings-modal">
        <h3 class="modal-title">{{ t('app.settings') }}</h3>

        <div class="settings-section">
          <div class="settings-label">{{ t('settings.language') }}</div>
          <div class="settings-radio-row">
            <label class="toggle-row">
              <input v-model="settings.language" type="radio" value="zh" class="toggle-check" />
              <span class="toggle-text">中文</span>
            </label>
            <label class="toggle-row">
              <input v-model="settings.language" type="radio" value="en" class="toggle-check" />
              <span class="toggle-text">English</span>
            </label>
          </div>
        </div>

        <div class="settings-section">
          <div class="settings-label">{{ t('settings.theme') }}</div>
          <div class="settings-radio-row">
            <label class="toggle-row">
              <input v-model="settings.theme" type="radio" value="dark" class="toggle-check" />
              <span class="toggle-text">{{ t('settings.themeDark') }}</span>
            </label>
            <label class="toggle-row">
              <input v-model="settings.theme" type="radio" value="light" class="toggle-check" />
              <span class="toggle-text">{{ t('settings.themeLight') }}</span>
            </label>
            <label class="toggle-row">
              <input v-model="settings.theme" type="radio" value="system" class="toggle-check" />
              <span class="toggle-text">{{ t('settings.themeSystem') }}</span>
            </label>
          </div>
        </div>

        <p class="settings-placeholder">{{ t('settings.moreComing') }}</p>

        <div class="modal-actions">
          <button class="btn btn-primary" @click="close">{{ t('action.confirm') }}</button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.settings-modal {
  max-width: 460px;
  text-align: left;
}
.settings-section {
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.settings-section + .settings-section {
  margin-top: 10px;
}
.settings-label {
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.settings-radio-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.settings-placeholder {
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  margin-top: 10px;
}
</style>
