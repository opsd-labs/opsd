<script setup lang="ts">
defineProps<{
  label: string;
  hint?: string;
  required?: boolean;
}>();
</script>

<template>
  <div class="field">
    <label class="field-label">
      {{ label }}
      <span v-if="required" class="field-required">*</span>
    </label>
    <div class="field-body">
      <slot />
    </div>
    <p v-if="hint" class="field-hint">{{ hint }}</p>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 16px;
}
.field-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
}
.field-required {
  color: var(--accent);
  margin-left: 2px;
}
.field-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field-hint {
  font-size: 11px;
  color: var(--muted);
  margin: 0;
  line-height: 1.4;
}
</style>
