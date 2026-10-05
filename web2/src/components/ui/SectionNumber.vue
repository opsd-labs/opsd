<script setup lang="ts">
defineProps<{
  num: string | number;
}>();
</script>

<template>
  <span class="section-num" aria-hidden="true">
    {{ String(num).padStart(2, "0") }}
  </span>
</template>

<style scoped>
.section-num {
  font-size: 64px;
  font-weight: 700;
  line-height: 1;
  color: var(--line);
  user-select: none;
  letter-spacing: -0.02em;
}
</style>
