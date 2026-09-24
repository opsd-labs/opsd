<script setup lang="ts">
import { watch } from "vue";
import {
  DialogRoot,
  DialogPortal,
  DialogOverlay,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "reka-ui";
import Button from "./Button.vue";
import { X } from "lucide-vue-next";
const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    wide?: boolean;
    terminal?: boolean;
    description?: string;
  }>(),
  { description: "" },
);
defineEmits<{ "update:open": [value: boolean] }>();
let previous: HTMLElement | null = null;
watch(
  () => props.open,
  (value) => {
    if (value) previous = document.activeElement as HTMLElement;
  },
  { flush: "sync", immediate: true },
);
function restore(event: Event) {
  event.preventDefault();
  previous?.focus();
}
</script>
<template>
  <DialogRoot :open="open" @update:open="$emit('update:open', $event)"
    ><DialogPortal
      ><DialogOverlay class="ui-overlay" /><DialogContent
        class="ui-drawer"
        :class="{ 'is-wide': wide, 'is-terminal': terminal }"
        @close-auto-focus="restore"
        ><header class="drawer-header">
          <DialogTitle>{{ title }}</DialogTitle
          ><Button
            icon
            variant="text"
            aria-label="关闭对话框"
            @click="$emit('update:open', false)"
            ><X
          /></Button>
        </header>
        <DialogDescription
          :class="description ? 'drawer-description' : 'sr-only'"
          >{{ description || title }}</DialogDescription
        >
        <div class="drawer-body"><slot /></div>
        <footer v-if="$slots.footer" class="drawer-footer">
          <slot name="footer" /></footer></DialogContent></DialogPortal
  ></DialogRoot>
</template>
