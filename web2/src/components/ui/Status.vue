<script setup lang="ts">
import type { StatusTone } from './marks';

defineProps<{
  tone: StatusTone;
  text?: string;
  mark?: string;
}>();
</script>

<template>
  <span :class="['sw-status', `sw-status--${tone}`]">
    <span class="sw-status__dot" />
    <span v-if="text" class="sw-status__text">{{ text }}</span>
  </span>
</template>

<style scoped>
.sw-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: var(--text-table);
  line-height: 1;
  white-space: nowrap;
}

.sw-status__dot {
  width: 6px;
  height: 6px;
  flex-shrink: 0;
}

.sw-status--success .sw-status__dot { background: var(--positive); }
.sw-status--success .sw-status__text { color: var(--positive); }

.sw-status--warning .sw-status__dot { background: var(--warning); }
.sw-status--warning .sw-status__text { color: var(--warning); }

.sw-status--danger .sw-status__dot { background: var(--danger); }
.sw-status--danger .sw-status__text { color: var(--danger); }

.sw-status--neutral .sw-status__dot { background: var(--ink-3); }
.sw-status--neutral .sw-status__text { color: var(--ink-3); }
</style>
