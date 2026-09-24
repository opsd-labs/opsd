<script setup lang="ts">
/** 归属标签：中性细边、静态外观，默认光标；用于节点、维护来源与只读属性。 */
defineProps<{ title?: string }>();
</script>
<template>
  <span class="affiliation" :title="title"><slot /></span>
</template>
