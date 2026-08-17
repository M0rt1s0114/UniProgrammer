<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'

export interface UiOption {
  value: string | number
  label: string
}

const props = withDefaults(
  defineProps<{
    modelValue: string | number
    options: UiOption[]
    placeholder?: string
    disabled?: boolean
  }>(),
  {
    placeholder: '',
    disabled: false,
  },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string | number): void
  (e: 'change', value: string | number): void
}>()

const open = ref(false)
const rootRef = ref<HTMLElement | null>(null)

const currentLabel = computed(() => {
  const found = props.options.find((o) => o.value === props.modelValue)
  return found?.label ?? props.placeholder ?? ''
})

function toggle() {
  if (props.disabled) return
  open.value = !open.value
}

function select(option: UiOption) {
  open.value = false
  emit('update:modelValue', option.value)
  emit('change', option.value)
}

function onDocumentMouseDown(e: MouseEvent) {
  if (rootRef.value && !rootRef.value.contains(e.target as Node)) {
    open.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') open.value = false
}

watch(
  () => props.disabled,
  (v) => {
    if (v) open.value = false
  },
)

onMounted(() => {
  document.addEventListener('mousedown', onDocumentMouseDown)
  document.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onDocumentMouseDown)
  document.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div ref="rootRef" class="ui-select" :class="{ 'is-open': open, 'is-disabled': disabled }">
    <button type="button" class="ui-select-trigger input" :disabled="disabled" @click="toggle">
      <span class="ui-select-label">{{ currentLabel }}</span>
      <span class="ui-select-arrow">▾</span>
    </button>
    <Transition name="drop">
      <div v-if="open" class="ui-select-menu">
        <div
          v-for="option in options"
          :key="String(option.value)"
          class="ui-select-item"
          :class="{ 'is-selected': option.value === modelValue }"
          @click="select(option)"
        >
          {{ option.label }}
        </div>
        <div v-if="options.length === 0" class="ui-select-empty">{{ placeholder }}</div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.ui-select {
  position: relative;
  width: 100%;
}

.ui-select-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  cursor: pointer;
  text-align: left;
}
.ui-select-trigger:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.ui-select-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-select-arrow {
  color: var(--text-muted);
  font-size: 10px;
  line-height: 1;
}

.ui-select-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 100;
  max-height: 240px;
  overflow-y: auto;
  background: var(--bg-elevated);
  border: 1px solid var(--border-accent);
  border-radius: var(--radius-md);
  box-shadow: 0 10px 24px rgba(0, 0, 0, 0.55);
  padding: 4px;
}

.ui-select-item {
  padding: 6px 8px;
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ui-select-item:hover {
  background: var(--accent-subtle);
  color: var(--text-primary);
}
.ui-select-item.is-selected {
  color: var(--accent);
}

.ui-select-empty {
  padding: 6px 8px;
  font-size: 11px;
  color: var(--text-muted);
}

.drop-enter-active,
.drop-leave-active {
  transition:
    opacity 100ms ease,
    transform 100ms ease;
}
.drop-enter-from,
.drop-leave-to {
  opacity: 0;
  transform: translateY(-3px);
}
</style>
