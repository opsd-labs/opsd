<script setup lang="ts">
defineProps<{ modelValue: boolean }>();
defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>
<template>
  <label class="ui-checkbox"
    ><input
      type="checkbox"
      :checked="modelValue"
      @change="
        $emit('update:modelValue', ($event.target as HTMLInputElement).checked)
      " /><slot
  /></label>
</template>
