<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import type { LogEntry } from '@/stores/prog'
import { t } from '@/i18n'

const props = defineProps<{
  logs: LogEntry[]
}>()

const bottomRef = ref<HTMLElement | null>(null)
const autoScroll = ref(true)

watch(
  () => props.logs.length,
  async () => {
    if (autoScroll.value) {
      await nextTick()
      bottomRef.value?.scrollIntoView({ behavior: 'instant' })
    }
  },
)

function levelClass(level: LogEntry['level']) {
  return {
    info: 'log-info',
    warn: 'log-warn',
    error: 'log-error',
    success: 'log-success',
    functionTest: 'log-function-test',
  }[level]
}
</script>

<template>
  <div class="log-console">
    <!-- Inline mini-toolbar for auto-scroll (lightweight, no redundant title/clear) -->
    <div class="log-meta">
      <span class="log-count">{{ logs.length }} {{ t('log.lines') }}</span>
      <label class="autoscroll-toggle">
        <input v-model="autoScroll" type="checkbox" />
        {{ t('log.autoScroll') }}
      </label>
    </div>

    <div class="log-body">
      <div v-if="logs.length === 0" class="log-empty">
        {{ t('log.waiting') }}
      </div>
      <div v-for="entry in logs" :key="entry.id" class="log-line" :class="levelClass(entry.level)">
        <span class="log-time">{{ entry.time }}</span>
        <span v-if="entry.level === 'functionTest'" class="log-level-test">[FunctionTest]</span>
        <span class="log-msg">{{ entry.message }}</span>
      </div>
      <div ref="bottomRef" />
    </div>
  </div>
</template>

<style scoped>
.log-console {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-base);
  font-family: var(--font-mono);
  font-size: 12px;
}

.log-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 3px 10px;
  background: var(--bg-base);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.log-count {
  font-size: 11px;
  color: var(--text-muted);
  margin-right: auto;
}

.autoscroll-toggle {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--font-sans);
}

.autoscroll-toggle input {
  accent-color: var(--accent);
  cursor: pointer;
}

.log-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}

.log-empty {
  padding: 12px;
  color: var(--text-muted);
  font-family: var(--font-sans);
  font-size: 12px;
  text-align: center;
  margin-top: 12px;
}

.log-line {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 1px 10px;
  line-height: 1.6;
  border-left: 2px solid transparent;
}

.log-line:hover {
  background: var(--bg-surface);
}

.log-time {
  color: var(--text-muted);
  flex-shrink: 0;
  font-size: 11px;
  min-width: 52px;
  white-space: nowrap;
  padding-top: 1px;
}

.log-msg {
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-all;
}

.log-level-test {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 600;
  color: var(--test-purple);
  white-space: nowrap;
}

.log-info {
}
.log-warn {
  border-left-color: var(--color-warn);
}
.log-warn .log-msg {
  color: var(--color-warn);
}
.log-error {
  border-left-color: var(--color-danger);
}
.log-error .log-msg {
  color: var(--color-danger);
}
.log-success {
  border-left-color: var(--accent);
}
.log-function-test {
  border-left-color: var(--test-purple);
}
.log-function-test .log-msg {
  color: var(--test-purple);
}
.log-success .log-msg {
  color: var(--accent);
}
</style>
