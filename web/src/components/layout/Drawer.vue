<script setup lang="ts">
/**
 * Drawer — 右侧滑入抽屉
 * 用于操作表单、详情、计划确认
 */
export interface DrawerProps {
  open: boolean
  title?: string
  size?: 'default' | 'lg'
}

const props = defineProps<DrawerProps>()
const emit = defineEmits<{ (e: 'close'): void }>()

function onOverlayClick() {
  emit('close')
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
</script>

<template>
  <Teleport to="body">
    <!-- 背景遮罩 -->
    <Transition name="overlay">
      <div
        v-if="open"
        class="drawer-overlay"
        aria-hidden="true"
        @click="onOverlayClick"
      />
    </Transition>

    <!-- 抽屉面板 -->
    <Transition name="drawer">
      <div
        v-if="open"
        class="drawer"
        :class="size === 'lg' ? 'drawer--lg' : ''"
        role="dialog"
        :aria-modal="true"
        :aria-label="title"
        tabindex="-1"
        @keydown="onKeydown"
      >
        <!-- 头部 -->
        <div v-if="title" class="drawer__header">
          <h2 class="drawer__title">{{ title }}</h2>
          <button
            class="drawer__close"
            aria-label="关闭"
            @click="emit('close')"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                 stroke-linejoin="round" aria-hidden="true">
              <path d="M18 6 6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <!-- 内容区域 -->
        <div class="drawer__body">
          <slot />
        </div>

        <!-- 底部操作 -->
        <div v-if="$slots.footer" class="drawer__footer">
          <slot name="footer" />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* ---- 遮罩 ---- */
.drawer-overlay {
  position: fixed;
  inset: 0;
  background: var(--color-overlay);
  z-index: calc(var(--z-drawer) - 1);
}

.overlay-enter-active,
.overlay-leave-active {
  transition: opacity var(--duration-base) var(--ease-out);
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}

/* ---- 抽屉 ---- */
.drawer {
  position: fixed;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(var(--drawer-w), 100vw);
  background: var(--color-surface);
  box-shadow: var(--shadow-lg);
  z-index: var(--z-drawer);
  display: flex;
  flex-direction: column;
  outline: none;
}

.drawer--lg {
  width: min(var(--drawer-w-lg), 100vw);
}

.drawer-enter-active,
.drawer-leave-active {
  transition: transform var(--duration-slow) var(--ease-out);
}

.drawer-enter-from,
.drawer-leave-to {
  transform: translateX(100%);
}

/* ---- 头部 ---- */
.drawer__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--topbar-h);
  padding-inline: var(--sp-5);
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
}

.drawer__title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  color: var(--color-ink);
}

.drawer__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: var(--control-md);
  height: var(--control-md);
  background: transparent;
  border: none;
  border-radius: var(--radius-md);
  color: var(--color-muted);
  cursor: pointer;
  transition: background-color var(--duration-fast) var(--ease-out),
              color var(--duration-fast) var(--ease-out);
}

.drawer__close:hover {
  background: var(--color-hover-bg);
  color: var(--color-ink);
}

.drawer__close:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

/* ---- 内容 ---- */
.drawer__body {
  flex: 1;
  overflow-y: auto;
  padding: var(--sp-5);
}

/* ---- 底部 ---- */
.drawer__footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--sp-2);
  padding: var(--sp-4) var(--sp-5);
  border-top: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
}
</style>
