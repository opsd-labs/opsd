<script setup lang="ts">
// Input 单行输入框

export interface InputProps {
  modelValue?: string
  type?: string
  placeholder?: string
  disabled?: boolean
  readonly?: boolean
  error?: string
  size?: 'sm' | 'md'
}

const props = withDefaults(defineProps<InputProps>(), {
  type: 'text',
  size: 'md',
})

const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void
}>()
</script>

<template>
  <div class="input-wrap" :class="{ 'input-wrap--error': !!error }">
    <input
      class="input"
      :class="[`input--${size}`, { 'input--error': !!error }]"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :readonly="readonly"
      :aria-invalid="!!error"
      :aria-describedby="error ? 'input-error' : undefined"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
    <p v-if="error" id="input-error" class="input-error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped>
.input-wrap {
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
}

.input {
  width: 100%;
  height: var(--control-md);
  padding-inline: var(--sp-3);
  background: var(--color-surface);
  color: var(--color-ink);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-family: inherit;
  outline: none;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.input::placeholder {
  color: var(--color-placeholder);
}

.input:focus-visible {
  border-color: var(--color-accent);
  box-shadow: var(--shadow-focus);
}

.input--error {
  border-color: var(--color-danger);
}

.input--error:focus-visible {
  box-shadow: 0 0 0 2px var(--color-danger);
}

.input--sm {
  height: var(--control-sm);
  padding-inline: var(--sp-2);
  font-size: var(--text-xs);
}

.input-error {
  font-size: var(--text-xs);
  color: var(--color-danger);
  line-height: var(--leading-tight);
}
</style>
