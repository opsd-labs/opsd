<script setup lang="ts">
import Button from "./Button.vue";
import { X } from "lucide-vue-next";
withDefaults(
  defineProps<{
    tone?: "neutral" | "warning" | "danger";
    dismissible?: boolean;
  }>(),
  { tone: "neutral" },
);
defineEmits<{ dismiss: [] }>();
</script>
<template>
  <div
    class="ui-notice"
    :class="`tone-${tone}`"
    :role="tone === 'danger' ? 'alert' : 'status'"
  >
    <span><slot /></span
    ><Button
      v-if="dismissible"
      icon
      variant="text"
      aria-label="关闭提示"
      @click="$emit('dismiss')"
      ><X
    /></Button>
  </div>
</template>
