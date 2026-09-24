<script setup lang="ts">
defineProps<{
  modelValue?: string | number;
  options: { value: string | number; label: string }[];
}>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>
<template>
  <select
    class="ui-select"
    :value="modelValue"
    @change="
      emit('update:modelValue', ($event.target as HTMLSelectElement).value)
    "
  >
    <option v-for="option in options" :key="option.value" :value="option.value">
      {{ option.label }}
    </option>
  </select>
</template>
