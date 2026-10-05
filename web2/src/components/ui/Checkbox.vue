<script setup lang="ts">
defineProps<{
  modelValue?: boolean;
  label?: string;
  disabled?: boolean;
}>();

defineEmits<{
  'update:modelValue': [value: boolean];
}>();
</script>

<template>
  <label class="sw-checkbox" :class="{ 'is-disabled': disabled }">
    <input
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="$emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
    />
    <span class="sw-checkbox__box" />
    <span v-if="label" class="sw-checkbox__label">{{ label }}</span>
    <slot />
  </label>
</template>

<style scoped>
.sw-checkbox {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  cursor: pointer;
  font-size: var(--text-table);
  line-height: 1;
}

.sw-checkbox input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.sw-checkbox__box {
  width: 14px;
  height: 14px;
  border: 1px solid var(--line-strong);
  background: transparent;
  flex-shrink: 0;
  position: relative;
}

.sw-checkbox input:checked + .sw-checkbox__box {
  background: var(--ink);
  border-color: var(--ink);
}

.sw-checkbox input:checked + .sw-checkbox__box::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 4px;
  width: 4px;
  height: 7px;
  border: solid var(--bg);
  border-width: 0 1.5px 1.5px 0;
  transform: rotate(45deg);
}

.sw-checkbox input:focus-visible + .sw-checkbox__box {
  outline: var(--focus);
  outline-offset: -2px;
}

.sw-checkbox.is-disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.sw-checkbox__label {
  color: var(--ink);
}
</style>
