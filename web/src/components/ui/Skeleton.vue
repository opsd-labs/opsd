<script setup lang="ts">
/**
 * Skeleton — 加载骨架块
 * CSS shimmer 扫光动画
 */
export interface SkeletonProps {
  width?: string
  height?: string
  circle?: boolean
  lines?: number   // 多行文字骨架
}

const props = withDefaults(defineProps<SkeletonProps>(), {
  height: '16px',
})
</script>

<template>
  <!-- 多行骨架 -->
  <div v-if="lines && lines > 1" class="skeleton-lines">
    <div
      v-for="i in lines"
      :key="i"
      class="skeleton"
      :style="{
        width: i === lines ? '65%' : '100%',
        height,
        borderRadius: 'var(--radius-sm)',
      }"
    />
  </div>

  <!-- 单块骨架 -->
  <div
    v-else
    class="skeleton"
    :style="{
      width: width ?? '100%',
      height,
      borderRadius: circle ? 'var(--radius-full)' : 'var(--radius-sm)',
    }"
    aria-hidden="true"
  />
</template>

<style scoped>
@keyframes shimmer {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton {
  display: block;
  background: linear-gradient(
    90deg,
    var(--color-border-subtle) 25%,
    var(--color-hover-bg)      50%,
    var(--color-border-subtle) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 1.5s ease-in-out infinite;
  flex-shrink: 0;
}

.skeleton-lines {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
</style>
