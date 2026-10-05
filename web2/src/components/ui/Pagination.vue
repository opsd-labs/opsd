<script setup lang="ts">
const props = defineProps<{
  page: number;
  pageSize: number;
  total: number;
}>();

const emit = defineEmits<{ "update:page": [page: number] }>();

function pages() {
  const total = Math.max(1, Math.ceil(props.total / props.pageSize));
  const result: (number | "...")[] = [];
  if (total <= 7) {
    for (let i = 1; i <= total; i++) result.push(i);
    return result;
  }
  result.push(1);
  if (props.page > 3) result.push("...");
  const start = Math.max(2, props.page - 1);
  const end = Math.min(total - 1, props.page + 1);
  for (let i = start; i <= end; i++) result.push(i);
  if (props.page < total - 2) result.push("...");
  result.push(total);
  return result;
}

function go(p: number) {
  if (p >= 1 && p <= Math.ceil(props.total / props.pageSize)) {
    emit("update:page", p);
  }
}
</script>

<template>
  <nav class="pagination" role="navigation" aria-label="分页">
    <button
      class="page-btn"
      :disabled="page <= 1"
      @click="go(page - 1)"
      aria-label="上一页"
    >
      ←
    </button>
    <template v-for="(p, i) in pages()" :key="i">
      <span v-if="p === '...'" class="page-ellipsis">…</span>
      <button
        v-else
        class="page-btn"
        :class="{ 'page-btn--active': p === page }"
        @click="go(p)"
      >
        {{ p }}
      </button>
    </template>
    <button
      class="page-btn"
      :disabled="page >= Math.ceil(total / pageSize)"
      @click="go(page + 1)"
      aria-label="下一页"
    >
      →
    </button>
  </nav>
</template>

<style scoped>
.pagination {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.page-btn {
  min-width: 28px;
  height: 28px;
  padding: 0 6px;
  background: none;
  border: 1px solid transparent;
  border-radius: var(--r);
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  cursor: pointer;
}
.page-btn:hover:not(:disabled) {
  border-color: var(--line);
}
.page-btn:disabled {
  color: var(--muted);
  cursor: not-allowed;
}
.page-btn--active {
  border-color: var(--ink);
  font-weight: 700;
}
.page-ellipsis {
  padding: 0 4px;
  font-size: 12px;
  color: var(--muted);
}
</style>
