<script setup lang="ts">
// 按钮组件
// variant: primary | default | ghost | danger | link
// size: sm | md | lg | icon
// busy: 显示 spinner 替换内容

export interface ButtonProps {
  variant?: 'primary' | 'default' | 'ghost' | 'danger' | 'link'
  size?: 'sm' | 'md' | 'lg' | 'icon'
  busy?: boolean
  disabled?: boolean
  type?: 'button' | 'submit' | 'reset'
}

const props = withDefaults(defineProps<ButtonProps>(), {
  variant: 'default',
  size: 'md',
  type: 'button',
})
</script>

<template>
  <button
    :type="props.type"
    class="btn"
    :class="[`btn--${variant}`, `btn--${size}`, { 'btn--busy': busy }]"
    :disabled="disabled || busy"
    :aria-disabled="disabled || busy"
    :aria-busy="busy"
  >
    <!-- busy spinner -->
    <span v-if="busy" class="btn__spinner" aria-hidden="true"></span>
    <!-- 内容（busy 时视觉隐藏，保持宽度） -->
    <span class="btn__content" :class="{ 'btn__content--hidden': busy }">
      <slot />
    </span>
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--sp-2);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  line-height: 1;
  white-space: nowrap;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  cursor: pointer;
  position: relative;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out),
    transform 60ms var(--ease-out);
  -webkit-user-select: none;
  user-select: none;
}

.btn:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.btn:active:not(:disabled) {
  transform: scale(0.98);
  filter: brightness(0.94);
}

/* ---- variant: primary ---- */
.btn--primary {
  background: var(--color-accent);
  color: var(--color-on-accent);
  border-color: var(--color-accent);
}

.btn--primary:hover:not(:disabled) {
  background: var(--color-accent-hover);
  border-color: var(--color-accent-hover);
}

/* ---- variant: default ---- */
.btn--default {
  background: var(--color-surface);
  color: var(--color-ink);
  border-color: var(--color-border);
}

.btn--default:hover:not(:disabled) {
  background: var(--color-hover-bg);
  border-color: var(--color-border);
}

/* ---- variant: ghost ---- */
.btn--ghost {
  background: transparent;
  color: var(--color-secondary);
  border-color: transparent;
}

.btn--ghost:hover:not(:disabled) {
  background: var(--color-hover-bg);
  color: var(--color-ink);
}

/* ---- variant: danger ---- */
.btn--danger {
  background: var(--color-surface);
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.btn--danger:hover:not(:disabled) {
  background: var(--color-danger-bg);
}

/* ---- variant: link ---- */
.btn--link {
  background: transparent;
  color: var(--color-accent);
  border-color: transparent;
  padding-inline: 0;
  height: auto;
  font-weight: var(--weight-normal);
}

.btn--link:hover:not(:disabled) {
  text-decoration: underline;
}

/* ---- sizes ---- */
.btn--sm {
  height: var(--control-sm);
  padding-inline: var(--sp-2);
  font-size: var(--text-xs);
}

.btn--md {
  height: var(--control-md);
  padding-inline: var(--sp-3);
}

.btn--lg {
  height: var(--control-lg);
  padding-inline: var(--sp-5);
  font-size: var(--text-md);
}

/* 正方形图标按钮 */
.btn--icon {
  height: var(--control-md);
  width: var(--control-md);
  padding: 0;
}

/* ---- busy state ---- */
.btn__spinner {
  position: absolute;
  width: 14px;
  height: 14px;
  border: 1.5px solid currentColor;
  border-top-color: transparent;
  border-radius: var(--radius-full);
  animation: btn-spin 600ms linear infinite;
  opacity: 0.7;
}

.btn__content--hidden {
  opacity: 0;
  pointer-events: none;
}

@keyframes btn-spin {
  to { transform: rotate(360deg); }
}
</style>
