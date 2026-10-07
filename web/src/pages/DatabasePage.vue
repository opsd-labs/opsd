<script setup lang="ts">
/** DatabasePage — 只读数据库巡检结果 */
import { computed } from 'vue'
import { workspace } from '../state/workspace'

const node = computed(() => workspace.selectedNode.value)
</script>

<template>
  <div class="db-page">
    <div v-if="!node?.capabilities.database" class="db-page__unavail">
      当前节点不支持数据库巡检或节点离线
    </div>
    <div v-else class="db-page__placeholder">
      数据库巡检结果（只读展示，实现中）
    </div>
  </div>
</template>

<style scoped>
.db-page { display: flex; flex-direction: column; gap: var(--sp-4); }
.db-page__unavail, .db-page__placeholder {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}
</style>
