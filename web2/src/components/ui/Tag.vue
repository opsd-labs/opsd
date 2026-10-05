<script setup lang="ts">
defineProps<{
  text?: string;
  variant?: 'default' | 'accent';
}>();
</script>

<template>
  <span :class="['sw-tag', variant ? `sw-tag--${variant}` : '']"><slot>{{ text }}</slot></span>
</template>

<style scoped>
.sw-tag {
  display: inline-block;
  font-size: var(--text-small);
  line-height: 1;
  padding: 2px 6px;
  border: 1px solid var(--line);
  color: var(--ink-2);
  white-space: nowrap;
}

.sw-tag--accent {
  border-color: var(--accent);
  color: var(--accent);
}
</style>
