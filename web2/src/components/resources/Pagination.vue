<script setup lang="ts">
import Button from "../ui/Button.vue";

defineProps<{ page: number; pages: number; total: number; size: number }>();
defineEmits<{ "update:page": [value: number] }>();
</script>

<template>
  <div class="table-pagination">
    <span class="table-pagination-info">共 {{ total }} 条 · {{ size }} 条/页</span>
    <div class="table-pagination-nav">
      <Button
        variant="icon"
        aria-label="上一页"
        :disabled="page <= 1"
        @click="$emit('update:page', page - 1)"
      >←</Button>
      <span class="table-pagination-current">{{ page }} / {{ pages }}</span>
      <Button
        variant="icon"
        aria-label="下一页"
        :disabled="page >= pages"
        @click="$emit('update:page', page + 1)"
      >→</Button>
    </div>
  </div>
</template>

<style scoped>
.table-pagination {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 0;
  font-size: 12px;
  color: var(--muted);
}
.table-pagination-info {
  font-variant-numeric: tabular-nums;
}
.table-pagination-nav {
  display: flex;
  align-items: center;
  gap: 8px;
}
.table-pagination-current {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--ink);
  min-width: 48px;
  text-align: center;
}
</style>
