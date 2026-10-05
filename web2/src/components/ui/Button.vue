<script setup lang="ts">
defineProps<{
  variant?: 'default' | 'primary' | 'danger' | 'text' | 'icon';
  disabled?: boolean;
  type?: 'button' | 'submit';
}>();
</script>

<template>
  <button
    :type="type || 'button'"
    :class="[
      'sw-btn',
      variant && variant !== 'default' ? `sw-btn--${variant}` : '',
    ]"
    :disabled="disabled"
  >
    <slot />
  </button>
</template>

<style scoped>
.sw-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: var(--control-h);
  padding: 0 var(--s4);
  font-family: var(--font);
  font-size: var(--text-table);
  font-weight: 400;
  line-height: 1;
  border: 1px solid var(--line-strong);
  background: transparent;
  color: var(--ink);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.1s;
}

.sw-btn:hover:not(:disabled) {
  background: var(--hover);
}

.sw-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.sw-btn--primary {
  background: var(--ink);
  color: var(--bg);
  border-color: var(--ink);
}

.sw-btn--primary:hover:not(:disabled) {
  background: var(--ink-2);
}

.sw-btn--danger {
  background: var(--accent);
  color: var(--on-accent);
  border-color: var(--accent);
}

.sw-btn--danger:hover:not(:disabled) {
  opacity: 0.9;
}

.sw-btn--text {
  border: none;
  padding: 0;
  height: auto;
  color: var(--ink);
  text-decoration: none;
}

.sw-btn--text:hover:not(:disabled) {
  text-decoration: underline;
  background: none;
}

.sw-btn--icon {
  border: none;
  padding: 0;
  width: var(--control-h);
  height: var(--control-h);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.sw-btn--icon:hover:not(:disabled) {
  background: var(--hover);
}
</style>
