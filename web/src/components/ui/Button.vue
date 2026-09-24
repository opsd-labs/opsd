<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
const props = withDefaults(
  defineProps<{
    variant?: "default" | "primary" | "danger" | "text";
    icon?: boolean;
    busy?: boolean;
    disabled?: boolean;
  }>(),
  { variant: "default" },
);
const element = ref<HTMLButtonElement>();
let restoreFocus = false;
watch(
  () => props.disabled,
  async (disabled) => {
    if (disabled) restoreFocus = document.activeElement === element.value;
    else if (restoreFocus) {
      await nextTick();
      if (document.activeElement === document.body) element.value?.focus();
      restoreFocus = false;
    }
  },
  { flush: "sync" },
);
</script>
<template>
  <button
    ref="element"
    type="button"
    class="ui-button"
    :class="[`is-${variant}`, { 'is-icon': icon }]"
    :disabled="disabled"
    :aria-busy="busy || undefined"
  >
    <slot />
  </button>
</template>
