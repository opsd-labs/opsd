<script setup lang="ts">
defineOptions({ inheritAttrs: false });
defineProps<{ modelValue?: string | number; multiline?: boolean }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>
<template>
  <textarea
    v-if="multiline"
    class="ui-input ui-textarea"
    v-bind="$attrs"
    :value="modelValue"
    @input="
      emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)
    "
  /><input
    v-else
    class="ui-input"
    v-bind="$attrs"
    :value="modelValue"
    @input="
      emit('update:modelValue', ($event.target as HTMLInputElement).value)
    "
  />
</template>
