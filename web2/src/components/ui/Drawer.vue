<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed } from "vue";

const props = defineProps<{
  open: boolean;
  side?: "left" | "right";
  width?: "normal" | "wide";
  title?: string;
  description?: string;
  wide?: boolean;
}>();

const emit = defineEmits<{ close: []; "update:open": [value: boolean] }>();
const panel = ref<HTMLElement>();

const isWide = computed(() => props.wide || props.width === "wide");

function onClose() {
  emit("close");
  emit("update:open", false);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") onClose();
}

onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));
watch(
  () => props.open,
  (v) => {
    document.body.style.overflow = v ? "hidden" : "";
  },
);
</script>

<template>
  <Teleport to="body">
    <Transition name="drawer">
      <div v-if="open" class="drawer-mask" @click.self="onClose" />
    </Transition>
    <Transition name="drawer">
      <div
        v-if="open"
        ref="panel"
        class="drawer"
        :class="[
          side === 'left' ? 'drawer--left' : 'drawer--right',
          isWide ? 'drawer--wide' : '',
        ]"
        role="dialog"
        aria-modal="true"
      >
        <div v-if="title" class="drawer-header">
          <div class="drawer-header__text">
            <h2 class="drawer-title">{{ title }}</h2>
            <p v-if="description" class="drawer-desc">{{ description }}</p>
          </div>
          <button class="drawer-close" type="button" aria-label="关闭" @click="onClose">✕</button>
        </div>
        <div class="drawer-body">
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.drawer-mask {
  position: fixed;
  inset: 0;
  background: var(--mask);
  z-index: 900;
}
.drawer {
  position: fixed;
  top: 0;
  bottom: 0;
  z-index: 901;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.drawer--right {
  right: 0;
  border-left: 1px solid var(--line);
  width: 480px;
  max-width: 100vw;
}
.drawer--left {
  left: 0;
  border-right: 1px solid var(--line);
  width: 480px;
  max-width: 100vw;
}
.drawer--wide {
  width: 720px;
}
.drawer-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 20px 24px 16px;
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}
.drawer-title {
  font-size: var(--text-heading);
  font-weight: 700;
  line-height: 1.2;
  margin: 0;
}
.drawer-desc {
  font-size: var(--text-meta);
  color: var(--ink-2);
  margin: 4px 0 0;
  line-height: 1.4;
}
.drawer-close {
  background: none;
  border: none;
  font-size: 14px;
  color: var(--ink-2);
  cursor: pointer;
  padding: 4px;
  line-height: 1;
}
.drawer-close:hover {
  color: var(--ink);
}
.drawer-body {
  flex: 1;
  overflow: auto;
  padding: 24px;
}
.drawer-enter-active,
.drawer-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}
.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}
.drawer--right.drawer-enter-from,
.drawer--right.drawer-leave-to {
  transform: translateX(100%);
}
.drawer--left.drawer-enter-from,
.drawer--left.drawer-leave-to {
  transform: translateX(-100%);
}
@media (max-width: 700px) {
  .drawer--right,
  .drawer--left,
  .drawer--wide {
    width: 100vw;
  }
}
</style>
