<script setup lang="ts">
import { ref } from 'vue'
import { useProgStore } from '@/stores/prog'
import { useSpiNor } from '@/services/spiNor'
import { t } from '@/i18n'

const store = useProgStore()
const spiNor = useSpiNor()

const showEraseConfirm = ref(false)

function requestErase() {
  showEraseConfirm.value = true
}

async function confirmErase() {
  showEraseConfirm.value = false
  await spiNor.eraseChip()
}
</script>

<template>
  <div class="toolbar">
    <div class="tool-group">
      <button class="tool-btn" :title="t('action.openFile')" @click="store.openFileViaDialog()">
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
            <path d="M3 10h18" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.openFile') }}</span>
      </button>

      <button
        class="tool-btn"
        :title="t('action.saveBin')"
        :disabled="!store.hexData"
        @click="spiNor.saveFileNative('bin')"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M5 3h11l3 3v15H5z" />
            <path d="M8 3v6h8V3" />
            <path d="M9 13h6M9 17h6" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.saveBin') }}</span>
      </button>

      <button
        class="tool-btn"
        :title="t('action.saveHex')"
        :disabled="!store.hexData"
        @click="spiNor.saveFileNative('hex')"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M6 2h9l4 4v16H6z" />
            <path d="M14 2v5h5" />
            <path d="M9 12h6M9 16h6" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.saveHex') }}</span>
      </button>
    </div>

    <div class="toolbar-divider" />

    <div class="tool-group tool-group-ops">
      <button
        class="tool-btn read-icon"
        :title="t('action.read')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.readChip()"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 3v10" />
            <path d="M8 9l4 4 4-4" />
            <path d="M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.read') }}</span>
      </button>

      <button
        class="tool-btn write-icon"
        :title="t('action.write')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.writeChip()"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 21V11" />
            <path d="M8 15l4-4 4 4" />
            <path d="M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.write') }}</span>
      </button>

      <button
        class="tool-btn erase-icon"
        :title="t('action.erase')"
        :disabled="!store.canOperate || store.isRunning"
        @click="requestErase"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M20 20H9L4 15a2 2 0 0 1 0-3l8-8a2 2 0 0 1 3 0l8 8a2 2 0 0 1 0 3l-5 5" />
            <path d="M6 17l5-5" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.erase') }}</span>
      </button>

      <button
        class="tool-btn verify-icon"
        :title="t('action.verify')"
        :disabled="!store.canOperate || store.isRunning"
        @click="spiNor.verifyChip()"
      >
        <span class="tool-icon">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 2l8 4v6c0 5-3.5 8-8 10-4.5-2-8-5-8-10V6z" />
            <path d="M9 12l2 2 4-4" />
          </svg>
        </span>
        <span class="tool-label">{{ t('action.verify') }}</span>
      </button>
    </div>

    <!-- 擦除确认弹窗 -->
    <Transition name="fade">
      <div v-if="showEraseConfirm" class="modal-backdrop" @click.self="showEraseConfirm = false">
        <div class="modal">
          <div class="modal-icon">
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path
                d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"
              />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
          </div>
          <h3 class="modal-title">{{ t('modal.eraseTitle') }}</h3>
          <p class="modal-body">{{ t('modal.eraseBody') }}</p>
          <div class="modal-actions">
            <button class="btn btn-secondary" @click="showEraseConfirm = false">
              {{ t('action.cancel') }}
            </button>
            <button class="btn btn-danger" @click="confirmErase">
              {{ t('action.confirmErase') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 58px;
  flex-shrink: 0;
  padding: 0 12px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
}

.tool-group {
  display: flex;
  align-items: center;
  gap: 4px;
}

.tool-group-ops {
  margin-left: auto;
}

.toolbar-divider {
  width: 1px;
  height: 28px;
  background: var(--border);
  flex-shrink: 0;
}

.tool-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  min-width: 54px;
  height: 46px;
  padding: 4px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  cursor: pointer;
  transition:
    background 120ms,
    border-color 120ms,
    color 120ms;
}

.tool-btn:hover:not(:disabled) {
  background: var(--bg-elevated);
  border-color: var(--border);
  color: var(--text-primary);
}

.tool-btn:active:not(:disabled) {
  background: var(--bg-overlay);
}

.tool-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.tool-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.tool-label {
  font-size: 10px;
  line-height: 1;
  white-space: nowrap;
}

.read-icon:hover:not(:disabled) {
  color: var(--color-info);
}

.write-icon:hover:not(:disabled) {
  color: var(--accent);
}

.erase-icon:hover:not(:disabled) {
  color: var(--color-danger);
}

.verify-icon:hover:not(:disabled) {
  color: var(--color-warn);
}
</style>
