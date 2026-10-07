<script setup lang="ts">
import { reactive } from 'vue'
import { provide } from 'vue'
import { TOAST_KEY } from '../../composables/useToast'
import type { ToastItem, ToastApi } from '../../composables/useToast'
import ToastItemVue from './ToastItem.vue'

const toasts = reactive<ToastItem[]>([])

let counter = 0

function add(type: ToastItem['type'], message: string, duration: number): void {
  const id = String(++counter)
  toasts.push({ id, type, message, duration })
  if (duration > 0) {
    setTimeout(() => dismiss(id), duration)
  }
}

function dismiss(id: string): void {
  const idx = toasts.findIndex(t => t.id === id)
  if (idx !== -1) toasts.splice(idx, 1)
}

const api: ToastApi = {
  success: (msg, duration = 4000) => add('success', msg, duration),
  error:   (msg, duration = 0)    => add('error',   msg, duration),
  warning: (msg, duration = 5000) => add('warning', msg, duration),
  info:    (msg, duration = 4000) => add('info',    msg, duration),
  dismiss,
}

provide(TOAST_KEY, api)
</script>

<template>
  <slot />

  <!-- Toast 容器：右下角固定定位 -->
  <Teleport to="body">
    <div class="toast-container" role="region" aria-label="通知">
      <TransitionGroup name="toast" tag="div" class="toast-list">
        <ToastItemVue
          v-for="t in toasts"
          :key="t.id"
          :item="t"
          @dismiss="dismiss(t.id)"
        />
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-container {
  position: fixed;
  bottom: var(--sp-5);
  right: var(--sp-5);
  z-index: var(--z-toast);
  pointer-events: none;
}

.toast-list {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  align-items: flex-end;
}

/* TransitionGroup 动画 */
.toast-enter-active {
  transition: opacity var(--duration-base) var(--ease-out),
              transform var(--duration-base) var(--ease-out);
}

.toast-leave-active {
  transition: opacity var(--duration-fast) var(--ease-out),
              transform var(--duration-fast) var(--ease-out);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(12px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(12px);
}
</style>
