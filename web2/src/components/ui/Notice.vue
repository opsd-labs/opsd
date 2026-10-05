<script setup lang="ts">
defineProps<{
  tone?: 'info' | 'warning' | 'danger';
}>();
</script>

<template>
  <div :class="['sw-notice', tone ? `sw-notice--${tone}` : '']">
    <slot />
  </div>
</template>

<style scoped>
.sw-notice {
  padding: var(--s3) var(--s4);
  border-left: 3px solid var(--ink);
  font-size: var(--text-table);
  line-height: 1.5;
  color: var(--ink-2);
}

.sw-notice--info {
  border-left-color: var(--ink);
}

.sw-notice--warning {
  border-left-color: var(--warning);
  background: var(--warning-bg);
}

.sw-notice--danger {
  border-left-color: var(--accent);
  background: var(--danger-bg);
}
</style>
