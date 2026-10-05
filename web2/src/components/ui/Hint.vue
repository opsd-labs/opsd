<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";

defineProps<{
  text: string;
  side?: "top" | "bottom" | "left" | "right";
}>();

const show = ref(false);
const el = ref<HTMLElement>();
let timer = 0;

function enter() {
  clearTimeout(timer);
  timer = window.setTimeout(() => (show.value = true), 300);
}
function leave() {
  clearTimeout(timer);
  show.value = false;
}

onMounted(() => {
  el.value?.addEventListener("mouseenter", enter);
  el.value?.addEventListener("mouseleave", leave);
  el.value?.addEventListener("focus", enter);
  el.value?.addEventListener("blur", leave);
});
onBeforeUnmount(() => {
  clearTimeout(timer);
});
</script>

<template>
  <span ref="el" class="hint-wrap" tabindex="0">
    <slot />
    <Transition name="hint">
      <span v-if="show" class="hint" :class="`hint--${side || 'top'}`" role="tooltip">
        {{ text }}
      </span>
    </Transition>
  </span>
</template>

<style scoped>
.hint-wrap {
  position: relative;
  display: inline-flex;
}
.hint {
  position: absolute;
  padding: 4px 8px;
  background: var(--ink);
  color: var(--surface);
  font-size: 11px;
  line-height: 1.4;
  white-space: nowrap;
  pointer-events: none;
  z-index: 1000;
  border-radius: var(--r);
}
.hint--top {
  bottom: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
}
.hint--bottom {
  top: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
}
.hint--left {
  right: calc(100% + 6px);
  top: 50%;
  transform: translateY(-50%);
}
.hint--right {
  left: calc(100% + 6px);
  top: 50%;
  transform: translateY(-50%);
}
.hint-enter-active,
.hint-leave-active {
  transition: opacity 0.1s ease;
}
.hint-enter-from,
.hint-leave-to {
  opacity: 0;
}
</style>
