<script setup lang="ts">
/**
 * NodesPage — 单节点深度视图
 * 指标曲线 + 文件管理 + 宿主机 Shell
 */
import { computed } from 'vue'
import { workspace } from '../state/workspace'
import { formatPercent, formatBytes, formatUptime } from '../utils/format'
import Status from '../components/ui/Status.vue'

const node = computed(() => workspace.selectedNode.value)
const metrics = computed(() =>
  node.value ? workspace.state.metrics[node.value.id] : null
)
</script>

<template>
  <div class="nodes-page">
    <div v-if="!node" class="nodes-page__empty">
      <span>请选择一个节点</span>
    </div>

    <template v-else>
      <!-- 节点头部 -->
      <div class="node-header">
        <div class="node-header__info">
          <h2 class="node-header__name">{{ node.name }}</h2>
          <code class="node-header__addr">{{ node.address }}</code>
        </div>
        <Status :tone="node.connected ? 'success' : 'danger'"
                :label="node.connected ? '在线' : '离线'" />
      </div>

      <!-- 指标卡片（占位，完整实现在 MetricsChart） -->
      <div v-if="metrics && !metrics.error" class="metrics-grid">
        <div class="metric-card">
          <span class="metric-card__label">CPU</span>
          <span class="metric-card__value">{{ formatPercent(metrics.cpu_percent) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">内存</span>
          <span class="metric-card__value">{{ formatPercent(metrics.mem_percent) }}</span>
          <span class="metric-card__sub">{{ formatBytes(metrics.mem_used_bytes) }} / {{ formatBytes(metrics.mem_total_bytes) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">磁盘</span>
          <span class="metric-card__value">{{ formatPercent(metrics.disk_percent) }}</span>
          <span class="metric-card__sub">{{ formatBytes(metrics.disk_used_bytes) }} / {{ formatBytes(metrics.disk_total_bytes) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">运行时间</span>
          <span class="metric-card__value">{{ formatUptime(metrics.uptime_secs) }}</span>
        </div>
      </div>

      <div v-else-if="!node.connected" class="nodes-page__offline">
        节点离线，指标不可用
      </div>
    </template>
  </div>
</template>

<style scoped>
.nodes-page { display: flex; flex-direction: column; gap: var(--sp-5); }
.nodes-page__empty, .nodes-page__offline {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}

.node-header {
  display: flex; align-items: center;
  justify-content: space-between; gap: var(--sp-4);
}
.node-header__info { display: flex; flex-direction: column; gap: var(--sp-1); }
.node-header__name { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--color-ink); }
.node-header__addr { font-size: var(--text-sm); color: var(--color-muted); font-family: var(--font-mono); }

.metrics-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--sp-3);
}
.metric-card {
  display: flex; flex-direction: column; gap: var(--sp-1);
  padding: var(--sp-4); background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg); box-shadow: var(--shadow-sm);
}
.metric-card__label { font-size: var(--text-xs); color: var(--color-muted); font-weight: var(--weight-medium); }
.metric-card__value { font-size: var(--text-xl); font-weight: var(--weight-bold); color: var(--color-ink); font-family: var(--font-mono); }
.metric-card__sub { font-size: var(--text-xs); color: var(--color-muted); }
</style>
