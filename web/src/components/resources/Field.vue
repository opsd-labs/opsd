<script setup lang="ts">
defineProps<{ label: string; help?: string }>();
</script>
<template>
  <label class="resource-field"
    ><span>{{ label }}</span
    ><slot /><small v-if="help">{{ help }}</small></label
  >
</template>
