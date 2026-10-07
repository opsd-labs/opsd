<script setup lang="ts">
/** StoragePage — 分布式存储集群管理 */
import { computed } from 'vue'
import { workspace } from '../state/workspace'

const node = computed(() => workspace.selectedNode.value)
</script>

<template>
  <div class="storage-page">
    <div v-if="!node?.capabilities.storage" class="storage-page__unavail">
      当前节点不支持存储管理或节点离线
    </div>
    <div v-else class="storage-page__placeholder">
      存储集群管理（实现中）
    </div>
  </div>
</template>

<style scoped>
.storage-page { display: flex; flex-direction: column; gap: var(--sp-4); }
.storage-page__unavail, .storage-page__placeholder {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}
</style>
