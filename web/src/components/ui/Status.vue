<script setup lang="ts">
// Status 语义标签（带图标）
// tone: success | warning | danger | neutral | info | unknown

export type StatusTone = 'success' | 'warning' | 'danger' | 'neutral' | 'info' | 'unknown'

export interface StatusProps {
  tone: StatusTone
  label: string
  /** 是否只显示图标（不显示文字，tooltip 用 label） */
  iconOnly?: boolean
}

const props = defineProps<StatusProps>()

// lucide 路径数据精简版，复杂图标直接嵌完整 path
const icons: Record<StatusTone, { viewBox: string; d: string }> = {
  success: { viewBox: '0 0 24 24', d: 'M12 2a10 10 0 1 1 0 20A10 10 0 0 1 12 2zm0 4a6 6 0 1 0 0 12A6 6 0 0 0 12 6z' },
  warning: { viewBox: '0 0 24 24', d: 'M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4m0 4h.01' },
  danger:  { viewBox: '0 0 24 24', d: 'M18 6 6 18M6 6l12 12' },
  neutral: { viewBox: '0 0 24 24', d: 'M5 12h14' },
  info:    { viewBox: '0 0 24 24', d: 'M12 16v-4m0-4h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0z' },
  unknown: { viewBox: '0 0 24 24', d: 'M9 9a3 3 0 0 1 6 0c0 2-3 3-3 3m.25 5h.01' },
}
</script>

<template>
  <span
    class="status"
    :class="`status--${tone}`"
    :title="iconOnly ? label : undefined"
    :aria-label="iconOnly ? label : undefined"
  >
    <!-- 语义图标 -->
    <svg
      class="status__icon"
      :viewBox="icons[tone].viewBox"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path :d="icons[tone].d" />
    </svg>
    <span v-if="!iconOnly" class="status__label">{{ label }}</span>
  </span>
</template>

<style scoped>
.status {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  line-height: 1;
  height: 22px;
  white-space: nowrap;
}

.status--success {
  color: var(--color-positive);
  background: var(--color-positive-bg);
}

.status--warning {
  color: var(--color-warning);
  background: var(--color-warning-bg);
}

.status--danger {
  color: var(--color-danger);
  background: var(--color-danger-bg);
}

.status--neutral {
  color: var(--color-neutral);
  background: var(--color-neutral-bg);
}

.status--info {
  color: var(--color-accent);
  background: var(--color-accent-subtle);
}

.status--unknown {
  color: var(--color-muted);
  background: var(--color-neutral-bg);
}

.status__icon {
  flex-shrink: 0;
}
</style>
