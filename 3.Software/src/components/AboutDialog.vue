<script setup lang="ts">
import { t } from '@/i18n'

const open = defineModel<boolean>('open', { default: false })

function close() {
  open.value = false
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="modal-backdrop" @click.self="close">
      <div class="modal about-modal">
        <h3 class="modal-title">{{ t('app.about') }}</h3>
        <p class="about-placeholder">{{ t('about.placeholder') }}</p>
        <div class="modal-actions">
          <button class="btn btn-primary" @click="close">{{ t('action.confirm') }}</button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.about-modal {
  max-width: 360px;
}
.about-placeholder {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.7;
  white-space: pre-line;
}
</style>
