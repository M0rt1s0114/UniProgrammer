<script setup lang="ts">
import { ref } from 'vue'
import { useProgStore } from '@/stores/prog'
import { useSpiNor } from '@/services/spiNor'
import { t } from '@/i18n'

const store = useProgStore()
const spiNor = useSpiNor()

// Toolbar icons supplied by the project owner.
// read / erase / verify: Font Awesome Free 7.3.1 (CC BY 4.0).
// write: Font Awesome Pro 7.3.1 asset — keep it out of public releases
// unless the project holds a redistribution license for it.

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
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M352 96C352 78.3 337.7 64 320 64C302.3 64 288 78.3 288 96L288 306.7L246.6 265.3C234.1 252.8 213.8 252.8 201.3 265.3C188.8 277.8 188.8 298.1 201.3 310.6L297.3 406.6C309.8 419.1 330.1 419.1 342.6 406.6L438.6 310.6C451.1 298.1 451.1 277.8 438.6 265.3C426.1 252.8 405.8 252.8 393.3 265.3L352 306.7L352 96zM160 384C124.7 384 96 412.7 96 448L96 480C96 515.3 124.7 544 160 544L480 544C515.3 544 544 515.3 544 480L544 448C544 412.7 515.3 384 480 384L433.1 384L376.5 440.6C345.3 471.8 294.6 471.8 263.4 440.6L206.9 384L160 384zM464 440C477.3 440 488 450.7 488 464C488 477.3 477.3 488 464 488C450.7 488 440 477.3 440 464C440 450.7 450.7 440 464 440z"
            />
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
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M352 173.3L352 384C352 401.7 337.7 416 320 416C302.3 416 288 401.7 288 384L288 173.3L246.6 214.7C234.1 227.2 213.8 227.2 201.3 214.7C188.8 202.2 188.8 181.9 201.3 169.4L297.3 73.4C309.8 60.9 330.1 60.9 342.6 73.4L438.6 169.4C451.1 181.9 451.1 202.2 438.6 214.7C426.1 227.2 405.8 227.2 393.3 214.7L352 173.3zM320 464C364.2 464 400 428.2 400 384L480 384C515.3 384 544 412.7 544 448L544 480C544 515.3 515.3 544 480 544L160 544C124.7 544 96 515.3 96 480L96 448C96 412.7 124.7 384 160 384L240 384C240 428.2 275.8 464 320 464zM464 488C477.3 488 488 477.3 488 464C488 450.7 477.3 440 464 440C450.7 440 440 450.7 440 464C440 477.3 450.7 488 464 488z"
            />
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
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M210.5 480L333.5 480L398.8 414.7L225.3 241.2L98.6 367.9L210.6 479.9zM256 544L210.5 544C193.5 544 177.2 537.3 165.2 525.3L49 409C38.1 398.1 32 383.4 32 368C32 352.6 38.1 337.9 49 327L295 81C305.9 70.1 320.6 64 336 64C351.4 64 366.1 70.1 377 81L559 263C569.9 273.9 576 288.6 576 304C576 319.4 569.9 334.1 559 345L424 480L544 480C561.7 480 576 494.3 576 512C576 529.7 561.7 544 544 544L256 544z"
            />
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
          <svg width="18" height="18" viewBox="0 0 640 640" fill="currentColor">
            <path
              d="M530.8 134.1C545.1 144.5 548.3 164.5 537.9 178.8L281.9 530.8C276.4 538.4 267.9 543.1 258.5 543.9C249.1 544.7 240 541.2 233.4 534.6L105.4 406.6C92.9 394.1 92.9 373.8 105.4 361.3C117.9 348.8 138.2 348.8 150.7 361.3L252.2 462.8L486.2 141.1C496.6 126.8 516.6 123.6 530.9 134z"
            />
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
