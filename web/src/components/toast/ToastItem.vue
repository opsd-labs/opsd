<script setup lang="ts">
import type { ToastItem } from '../../composables/useToast'

const props = defineProps<{ item: ToastItem }>()
const emit = defineEmits<{ (e: 'dismiss'): void }>()

// 图标名和颜色根据类型
const typeConfig = {
  success: { color: 'var(--color-positive)',  bg: 'var(--color-positive-bg)',  bar: 'var(--color-positive)' },
  error:   { color: 'var(--color-danger)',    bg: 'var(--color-danger-bg)',    bar: 'var(--color-danger)' },
  warning: { color: 'var(--color-warning)',   bg: 'var(--color-warning-bg)',   bar: 'var(--color-warning)' },
  info:    { color: 'var(--color-accent)',    bg: 'var(--color-surface)',      bar: 'var(--color-accent)' },
} as const
</script>

<template>
  <div
    class="toast"
    :style="{
      '--toast-bar': typeConfig[item.type].bar,
      background: typeConfig[item.type].bg,
    }"
    role="alert"
    aria-live="assertive"
  >
    <!-- 左侧语义色条 -->
    <div class="toast__bar" aria-hidden="true"></div>

    <div class="toast__body">
      <span class="toast__message">{{ item.message }}</span>
    </div>

    <button
      class="toast__close"
      aria-label="关闭通知"
      @click="emit('dismiss')"
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
        <path d="M1 1l12 12M13 1L1 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    </button>
  </div>
</template>

<style scoped>
.toast {
  display: flex;
  align-items: stretch;
  min-width: 280px;
  max-width: 400px;
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  border: 1px solid var(--color-border-subtle);
  overflow: hidden;
  pointer-events: all;
  cursor: default;
}

.toast__bar {
  width: 3px;
  flex-shrink: 0;
  background: var(--toast-bar);
}

.toast__body {
  flex: 1;
  padding: var(--sp-3) var(--sp-3);
  display: flex;
  align-items: center;
}

.toast__message {
  font-size: var(--text-sm);
  color: var(--color-ink);
  line-height: var(--leading-snug);
}

.toast__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  flex-shrink: 0;
  background: transparent;
  border: none;
  cursor: pointer;
  color: var(--color-muted);
  transition: color var(--duration-fast) var(--ease-out);
}

.toast__close:hover {
  color: var(--color-ink);
}

.toast__close:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
  border-radius: var(--radius-sm);
}
</style>
