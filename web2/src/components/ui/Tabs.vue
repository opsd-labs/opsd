<script setup lang="ts">
defineProps<{
  items: { value: string; label: string }[];
  modelValue: string;
}>();

defineEmits<{
  'update:modelValue': [value: string];
}>();
</script>

<template>
  <div class="sw-tabs" role="tablist">
    <button
      v-for="tab in items"
      :key="tab.value"
      role="tab"
      :aria-selected="modelValue === tab.value"
      :class="['sw-tabs__item', modelValue === tab.value ? 'is-active' : '']"
      @click="$emit('update:modelValue', tab.value)"
    >
      {{ tab.label }}
    </button>
  </div>
</template>

<style scoped>
.sw-tabs {
  display: flex;
  border-bottom: 1px solid var(--line);
  gap: 0;
}

.sw-tabs__item {
  padding: var(--s2) var(--s4);
  font-size: var(--text-table);
  font-weight: 400;
  color: var(--ink-3);
  border: none;
  background: none;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: color 0.1s, border-color 0.1s;
}

.sw-tabs__item:hover {
  color: var(--ink);
}

.sw-tabs__item.is-active {
  color: var(--ink);
  font-weight: 700;
  border-bottom-color: var(--ink);
}
</style>
