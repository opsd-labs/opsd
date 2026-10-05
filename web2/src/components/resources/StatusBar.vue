<script setup lang="ts">
defineProps<{
  items: Array<{ label: string; value: string | number }>;
}>();
</script>

<template>
  <div class="status-bar">
    <div v-for="(item, i) in items" :key="i" class="status-bar-item">
      <span class="status-bar-label">{{ item.label }}</span>
      <span class="status-bar-value">{{ item.value }}</span>
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 24px;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
  margin-bottom: 16px;
}
.status-bar-item {
  display: flex;
  align-items: baseline;
  gap: 6px;
}
.status-bar-label {
  font-size: 11px;
  color: var(--muted);
}
.status-bar-value {
  font-size: 13px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  color: var(--ink);
}
</style>
