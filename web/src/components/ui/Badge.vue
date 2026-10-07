<script setup lang="ts">
/**
 * Badge — 纯展示标签
 */
export type BadgeTone = 'default' | 'accent' | 'success' | 'warning' | 'danger'

export interface BadgeProps {
  tone?: BadgeTone
}

withDefaults(defineProps<BadgeProps>(), {
  tone: 'default',
})
</script>

<template>
  <span class="badge" :class="`badge--${tone}`">
    <slot />
  </span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  line-height: 1;
  height: 20px;
  white-space: nowrap;
}

.badge--default {
  background: var(--color-neutral-bg);
  color: var(--color-neutral);
}

.badge--accent {
  background: var(--color-accent-subtle);
  color: var(--color-accent);
}

.badge--success {
  background: var(--color-positive-bg);
  color: var(--color-positive);
}

.badge--warning {
  background: var(--color-warning-bg);
  color: var(--color-warning);
}

.badge--danger {
  background: var(--color-danger-bg);
  color: var(--color-danger);
}
</style>
