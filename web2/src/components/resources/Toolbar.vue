<script setup lang="ts">
defineProps<{
  label?: string;
  count?: number;
}>();
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-left">
      <span v-if="label" class="toolbar-label">{{ label }}</span>
      <span v-if="count !== undefined" class="toolbar-count">{{ count }}</span>
    </div>
    <div class="toolbar-right">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
  margin-bottom: 16px;
}
.toolbar-left {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.toolbar-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
}
.toolbar-count {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--ink);
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
