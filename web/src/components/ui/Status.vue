<script setup lang="ts">
import { computed } from "vue";
import { defaultMark, type StatusMark, type StatusTone } from "./marks";
const props = withDefaults(
  defineProps<{ tone?: StatusTone; mark?: StatusMark; dot?: boolean }>(),
  { tone: "neutral" },
);
/** 未显式指定符号时按语义色取默认符号；符号与文字共同表达，不依赖颜色。 */
const shape = computed(() => props.mark || defaultMark[props.tone]);
</script>
<template>
  <span class="ui-status" :class="[`tone-${tone}`, { 'has-dot': dot }]">
    <span v-if="dot" class="status-dot" aria-hidden="true" />
    <svg v-else class="status-mark" viewBox="0 0 16 16" aria-hidden="true">
      <circle v-if="shape === 'active'" cx="8" cy="8" r="3" fill="currentColor" stroke="none" />
      <template v-else-if="shape === 'paused'">
        <path d="M5.5 4v8M10.5 4v8" />
      </template>
      <template v-else-if="shape === 'pending'">
        <circle cx="8" cy="8" r="6" />
        <path d="M8 4.5v4M8 11.5h.01" />
      </template>
      <template v-else-if="shape === 'failed'">
        <circle cx="8" cy="8" r="6" />
        <path d="m6 6 4 4m0-4-4 4" />
      </template>
      <circle v-else-if="shape === 'unknown'" cx="8" cy="8" r="4.5" />
      <template v-else-if="shape === 'blocked'">
        <circle cx="8" cy="8" r="6" />
        <path d="M4.5 8h7" />
      </template>
      <path v-else d="M4 8h8" />
    </svg>
    <span><slot /></span>
  </span>
</template>
