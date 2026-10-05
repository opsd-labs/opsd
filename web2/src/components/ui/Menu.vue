<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";

defineProps<{
  items: Array<{ label: string; value: string; disabled?: boolean }>;
}>();

const model = defineModel<string>();
const open = ref(false);
const trigger = ref<HTMLElement>();

function toggle() {
  open.value = !open.value;
}

function select(value: string) {
  model.value = value;
  open.value = false;
}

function onClickOutside(e: MouseEvent) {
  if (!trigger.value?.contains(e.target as Node)) {
    open.value = false;
  }
}

onMounted(() => document.addEventListener("click", onClickOutside));
onBeforeUnmount(() => document.removeEventListener("click", onClickOutside));
</script>

<template>
  <div ref="trigger" class="menu-wrap">
    <button class="menu-trigger" @click="toggle" type="button">
      <slot name="trigger">
        <span>{{ items.find((i) => i.value === model)?.label || "—" }}</span>
        <svg
          class="menu-arrow"
          :class="{ 'menu-arrow--open': open }"
          width="10"
          height="10"
          viewBox="0 0 10 10"
        >
          <path d="M2 4 L5 7 L8 4" stroke="currentColor" stroke-width="1.5" fill="none" />
        </svg>
      </slot>
    </button>
    <div v-if="open" class="menu-panel" role="menu">
      <button
        v-for="item in items"
        :key="item.value"
        class="menu-item"
        :class="{ 'menu-item--active': item.value === model, 'menu-item--disabled': item.disabled }"
        role="menuitem"
        :disabled="item.disabled"
        @click="select(item.value)"
      >
        {{ item.label }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.menu-wrap {
  position: relative;
  display: inline-block;
}
.menu-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  cursor: pointer;
  white-space: nowrap;
}
.menu-trigger:hover {
  border-color: var(--ink);
}
.menu-arrow {
  transition: transform 0.15s ease;
}
.menu-arrow--open {
  transform: rotate(180deg);
}
.menu-panel {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  min-width: 100%;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
  z-index: 800;
  padding: 4px 0;
}
.menu-item {
  display: block;
  width: 100%;
  padding: 6px 12px;
  background: none;
  border: none;
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
}
.menu-item:hover {
  background: var(--bg);
}
.menu-item--active {
  font-weight: 700;
}
.menu-item--disabled {
  color: var(--muted);
  cursor: not-allowed;
}
</style>
