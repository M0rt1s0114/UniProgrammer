<script setup lang="ts">
import { computed } from 'vue'
import { useProgStore, formatBytes } from '@/stores/prog'
import { t } from '@/i18n'

const store = useProgStore()

const statusColor = computed(() => {
  if (store.detectStatus === 'programmer_fail' || store.detectStatus === 'chip_rule_fail')
    return '#f05050'
  if (store.status === 'running') return '#4a9eff'
  if (store.status === 'success') return '#00e5a0'
  return '#f05050'
})

const statusLabel = computed(() => {
  if (store.detectStatus === 'programmer_fail') return t('status.programmerFail')
  if (store.detectStatus === 'chip_rule_fail') return t('status.chipRuleFail')
  if (store.isRunning) return store.currentOp
  if (store.status === 'success') return t('status.connected')
  return t('status.disconnected')
})

const fileName = computed(() => {
  if (!store.filePath) return ''
  return store.filePath.split(/[\\/]/).pop() ?? store.filePath
})
</script>

<template>
  <div class="status-bar">
    <div class="status-left">
      <span class="status-dot" :style="{ color: statusColor }">
        <span class="dot" :class="{ pulse: store.status === 'running' }" />
      </span>
      <span class="status-label" :style="{ color: statusColor }">{{ statusLabel }}</span>
      <!-- 设备名称，仅连接后显示 -->
      <span
        v-if="store.status === 'success' && store.connectedDevice"
        class="device-name text-muted"
      >
        · {{ store.connectedDevice }}
      </span>
      <span v-if="store.vccOutputEnabled" class="vcc-badge">
        ⚡ {{ t('vcc.statusOn') }} {{ (store.vccTargetMv / 1000).toFixed(1) }}
        {{ t('vcc.voltageUnit') }}
      </span>
    </div>

    <div v-if="store.isRunning" class="status-progress">
      <div class="progress-track" style="width: 140px">
        <div class="progress-fill" :style="{ width: store.progress + '%' }" />
      </div>
      <span class="progress-pct">{{ Math.round(store.progress) }}%</span>
      <span class="progress-msg text-muted">{{ store.progressMessage }}</span>
    </div>

    <div class="status-right">
      <template v-if="fileName">
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style="color: var(--text-muted)"
        >
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14,2 14,8 20,8" />
        </svg>
        <span class="file-name">{{ fileName }}</span>
        <span v-if="store.fileSize" class="file-size text-muted">{{
          formatBytes(store.fileSize)
        }}</span>
      </template>
      <span v-else class="text-muted">{{ t('status.noFile') }}</span>
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 14px;
  height: 28px;
  background: var(--bg-surface);
  border-top: 1px solid var(--border);
  font-family: var(--font-mono);
  font-size: 11px;
  flex-shrink: 0;
}
.status-left {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 80px;
}
.status-dot {
  display: flex;
  align-items: center;
}
.status-label {
  font-weight: 500;
  font-size: 11px;
  font-family: var(--font-sans);
}
.device-name {
  font-family: var(--font-sans);
  font-size: 10px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}
.vcc-badge {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  color: #f05050;
  border: 1px solid rgba(240, 80, 80, 0.6);
  background: rgba(240, 80, 80, 0.1);
  border-radius: var(--radius-sm);
  padding: 1px 6px;
}
.status-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
}
.progress-pct {
  font-size: 11px;
  color: var(--text-secondary);
  width: 32px;
  text-align: right;
}
.progress-msg {
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 240px;
}
.status-right {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
}
.file-name {
  color: var(--text-secondary);
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-size {
  font-size: 10px;
}
</style>
