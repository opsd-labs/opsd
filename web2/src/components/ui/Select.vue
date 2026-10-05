<script setup lang="ts">
defineProps<{
  modelValue?: string;
  disabled?: boolean;
}>();

defineEmits<{
  'update:modelValue': [value: string];
}>();
</script>

<template>
  <select
    :value="modelValue"
    :disabled="disabled"
    class="sw-select"
    @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
  >
    <slot />
  </select>
</template>

<style scoped>
.sw-select {
  display: block;
  width: 100%;
  height: var(--control-h);
  padding: 0 var(--s5) 0 var(--s3);
  font-family: var(--font);
  font-size: var(--text-table);
  line-height: 1.4;
  background: transparent;
  border: 1px solid var(--line);
  color: var(--ink);
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6' fill='none' stroke='%23888' stroke-width='1'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 8px center;
}

.sw-select:focus {
  outline: none;
  border-color: var(--ink);
}

.sw-select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
