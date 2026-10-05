<script setup lang="ts">
defineProps<{
  modelValue?: string;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  underline?: boolean;
}>();

defineEmits<{
  'update:modelValue': [value: string];
}>();
</script>

<template>
  <input
    :type="type || 'text'"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    :class="['sw-input', underline ? 'sw-input--underline' : '']"
    @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
  />
</template>

<style scoped>
.sw-input {
  display: block;
  width: 100%;
  height: var(--control-h);
  padding: 0 var(--s3);
  font-family: var(--font);
  font-size: var(--text-table);
  line-height: 1.4;
  background: transparent;
  border: 1px solid var(--line);
  color: var(--ink);
  transition: border-color 0.1s;
}

.sw-input:focus {
  outline: none;
  border-color: var(--ink);
}

.sw-input::placeholder {
  color: var(--ink-3);
}

.sw-input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.sw-input--underline {
  border: none;
  border-bottom: 1px solid var(--line);
  padding: 0;
  border-radius: 0;
}

.sw-input--underline:focus {
  border-bottom-color: var(--ink);
}
</style>
