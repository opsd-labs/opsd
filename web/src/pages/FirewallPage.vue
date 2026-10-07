<script setup lang="ts">
/** FirewallPage — 防火墙规则管理（plan → apply 两步）*/
import { computed } from 'vue'
import { workspace } from '../state/workspace'

const node = computed(() => workspace.selectedNode.value)
const hasFirewall = computed(() => !!node.value?.capabilities.firewall)
</script>

<template>
  <div class="fw-page">
    <div v-if="!hasFirewall" class="fw-page__unavail">
      当前节点不支持防火墙管理或节点离线
    </div>
    <div v-else class="fw-page__placeholder">
      防火墙规则（plan → apply 流程，实现中）
    </div>
  </div>
</template>

<style scoped>
.fw-page { display: flex; flex-direction: column; gap: var(--sp-4); }
.fw-page__unavail, .fw-page__placeholder {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}
</style>
